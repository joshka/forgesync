//! # Stage and validate paginated child evidence
//!
//! Staging stores fetched pages under a reserved family observation. Page-set helpers load them,
//! count items, validate expected indexes, and merge members for finalization. Keeping those
//! checks here lets `finish` work from a coherent set rather than trusting a caller's page count.
//!
//! A page is provisional until the whole collection is complete. Repeated or interrupted provider
//! work must not expose staged rows as canonical child membership. The engine can record progress
//! while preserving the last complete view.

use sqlx::Row;

use super::{
    Archive, ArchiveLeaseToken, BTreeMap, EvidenceFamily, ObservationSequence, Serialize,
    SqliteConnection, StagedItem, StagedPage, StoreError, ThreadId, evidence_family_name,
    is_child_family, require_active_archive_lease, thread_row_id, to_sql_sequence,
};

impl Archive {
    /// Persists a provisional page without changing canonical child membership.
    ///
    /// Use the sequence returned by reservation and zero-based indexes from the provider traversal.
    /// Finalization validates the declared page set before publishing membership. A replay succeeds
    /// only when the same index has exactly the same serialized payload; conflicting payloads are
    /// rejected instead of silently replacing earlier evidence.
    ///
    /// This operation updates the generation's received-item count in the same transaction as the
    /// page write. An incomplete generation can accept further pages, but a completed, missing, or
    /// superseded generation cannot. Provider I/O must happen before this call.
    ///
    /// # Errors
    ///
    /// Returns an error for a read-only archive, unsupported family, unknown thread, stale or
    /// missing generation, conflicting replay, serialization failure, or database failure. An
    /// error does not expose provisional items as canonical membership.
    pub async fn stage_child_family_page<T>(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        sequence: ObservationSequence,
        page_index: u32,
        items: &[StagedItem<T>],
    ) -> Result<(), StoreError>
    where
        T: Serialize,
    {
        self.stage_child_family_page_inner(thread, family, sequence, page_index, items, None)
            .await
    }

    /// Stages a page while verifying the caller still owns the archive writer fence.
    ///
    /// See [`Self::stage_child_family_page`] for indexing, replay, and generation rules. The fence
    /// is checked inside the write transaction; losing ownership rejects the write before a page
    /// or count can commit. Use the same fence for reservation and finalization.
    ///
    /// # Errors
    ///
    /// Returns the unfenced operation's errors and stale or expired lease errors.
    pub async fn stage_child_family_page_fenced<T>(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        sequence: ObservationSequence,
        page_index: u32,
        items: &[StagedItem<T>],
        token: &ArchiveLeaseToken,
    ) -> Result<(), StoreError>
    where
        T: Serialize,
    {
        self.stage_child_family_page_inner(thread, family, sequence, page_index, items, Some(token))
            .await
    }

    /// Stages one page without changing canonical complete membership.
    async fn stage_child_family_page_inner<T>(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        sequence: ObservationSequence,
        page_index: u32,
        items: &[StagedItem<T>],
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<(), StoreError>
    where
        T: Serialize,
    {
        if !is_child_family(family) {
            return Err(StoreError::UnsupportedObservationFamily(
                evidence_family_name(family).to_owned(),
            ));
        }
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let family_name = evidence_family_name(family);
        let page_index = i64::from(page_index);
        let payload_json = serde_json::to_string(items)?;
        let mut transaction = writer.begin().await?;
        if let Some(token) = token {
            require_active_archive_lease(&mut transaction, token).await?;
        }
        let thread_row_id = thread_row_id(&mut transaction, thread).await?;
        let current_sequence: Option<i64> = sqlx::query_scalar(
            "SELECT sequence FROM thread_family_reservations WHERE thread_id = ? AND family = ?",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .fetch_optional(&mut *transaction)
        .await?;
        if current_sequence != Some(to_sql_sequence(sequence)?) {
            return Err(StoreError::StaleObservationGeneration);
        }

        let generation_status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM observation_generations WHERE thread_id = ? AND family = ? AND sequence = ?",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .bind(to_sql_sequence(sequence)?)
        .fetch_optional(&mut *transaction)
        .await?;
        match generation_status.as_deref() {
            None => return Err(StoreError::ObservationGenerationMissing),
            Some("complete") => return Err(StoreError::StaleObservationGeneration),
            Some("reserved" | "incomplete") => {}
            Some(_) => return Err(StoreError::ObservationGenerationMissing),
        }

        let existing_page: Option<String> = sqlx::query_scalar(
            "SELECT payload_json FROM observation_staging_pages WHERE thread_id = ? AND family = ? AND sequence = ? AND page_index = ?",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .bind(to_sql_sequence(sequence)?)
        .bind(page_index)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(existing_page) = existing_page {
            if existing_page != payload_json {
                return Err(StoreError::StagedPageConflict);
            }
            transaction.commit().await?;
            return Ok(());
        }

        sqlx::query(
            "INSERT INTO observation_staging_pages (thread_id, family, sequence, page_index, payload_json) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .bind(to_sql_sequence(sequence)?)
        .bind(page_index)
        .bind(payload_json)
        .execute(&mut *transaction)
        .await?;
        let staged_pages = load_staged_pages(
            &mut transaction,
            thread_row_id,
            family_name,
            to_sql_sequence(sequence)?,
        )
        .await?;
        let staged_count = count_staged_items(&staged_pages)?;
        sqlx::query(
            "UPDATE observation_generations SET status = 'reserved', received_items = ? WHERE thread_id = ? AND family = ? AND sequence = ?",
        )
        .bind(i64::try_from(staged_count).map_err(|_| StoreError::IntegerOutOfRange)?)
        .bind(thread_row_id)
        .bind(family_name)
        .bind(to_sql_sequence(sequence)?)
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
        Ok(())
    }
}

/// Loads pages for the reserved generation in page-number order.
pub async fn load_staged_pages(
    connection: &mut SqliteConnection,
    thread_row_id: i64,
    family: &str,
    sequence: i64,
) -> Result<Vec<StagedPage>, StoreError> {
    let rows = sqlx::query(
        "SELECT page_index, payload_json FROM observation_staging_pages WHERE thread_id = ? AND family = ? AND sequence = ? ORDER BY page_index",
    )
    .bind(thread_row_id)
    .bind(family)
    .bind(sequence)
    .fetch_all(&mut *connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            let index: i64 = row.try_get("page_index")?;
            let payload_json: String = row.try_get("payload_json")?;
            Ok(StagedPage {
                index,
                items: serde_json::from_str(&payload_json)?,
            })
        })
        .collect()
}

/// Counts distinct staged provider items before finalization.
pub fn count_staged_items(pages: &[StagedPage]) -> Result<u64, StoreError> {
    pages.iter().try_fold(0_u64, |count, page| {
        let item_count =
            u64::try_from(page.items.len()).map_err(|_| StoreError::IntegerOutOfRange)?;
        count
            .checked_add(item_count)
            .ok_or(StoreError::IntegerOutOfRange)
    })
}

/// Requires every declared page before a collection can become complete.
pub fn validate_page_set(pages: &[StagedPage], expected_pages: u32) -> Result<(), StoreError> {
    let found = u32::try_from(pages.len()).map_err(|_| StoreError::IntegerOutOfRange)?;
    if found != expected_pages {
        return Err(StoreError::IncompletePageSet {
            expected: expected_pages,
            found,
        });
    }
    for (index, page) in pages.iter().enumerate() {
        let expected_index = i64::try_from(index).map_err(|_| StoreError::IntegerOutOfRange)?;
        if page.index != expected_index {
            return Err(StoreError::IncompletePageSet {
                expected: expected_pages,
                found,
            });
        }
    }
    Ok(())
}

/// Rejects conflicting duplicate provider IDs within one generation.
pub fn merge_staged_items(
    pages: &[StagedPage],
) -> Result<BTreeMap<String, StagedItem<serde_json::Value>>, StoreError> {
    let mut items = BTreeMap::new();
    for item in pages.iter().flat_map(|page| &page.items) {
        let key = item.id.as_str().to_owned();
        if let Some(existing) = items.get(&key) {
            if existing != item {
                return Err(StoreError::StagedItemConflict);
            }
        } else {
            items.insert(key, item.clone());
        }
    }
    Ok(items)
}
