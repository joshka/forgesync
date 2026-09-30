//! # Stage and validate paginated child evidence
//!
//! Staging stores fetched pages under a reserved family observation. Page-set helpers load them,
//! count items, validate expected indexes, and merge members for finalization. Keeping those
//! checks here lets `finish` work from a coherent set rather than trusting a caller's page count.
//!
//! A page is provisional until the whole collection is complete. Repeated or interrupted provider
//! work must not expose staged rows as canonical child membership. The engine can record progress
//! while preserving the last complete view.
//!
//! `PageWrite` holds the exact SQL generation key through validation, replay comparison, and
//! insertion. The archive method owns commit. The page-set helpers below support finalization: a
//! received-item count includes duplicate IDs, while merging determines unique canonical members.

use std::collections::BTreeMap;

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::{ObservationSequence, ThreadId};
use serde::Serialize;
use sqlx::{Row, SqliteConnection};

use crate::archive::Archive;
use crate::error::StoreError;
use crate::families::StagedPage;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::observation_sql::{
    evidence_family_name, is_child_family, thread_row_id, to_sql_sequence,
};
use crate::observations::StagedItem;

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
        let page = PageWrite {
            thread: thread_row_id,
            family: family_name,
            sequence: to_sql_sequence(sequence)?,
            index: page_index,
            payload: payload_json,
        };
        page.validate_generation(&mut transaction).await?;
        if !page.is_replay(&mut transaction).await? {
            page.persist(&mut transaction).await?;
        }
        transaction.commit().await?;
        Ok(())
    }
}

/// A serialized page belonging to one exact reserved SQL generation.
///
/// This owner keeps the generation key identical across validation, replay comparison, insertion,
/// and count accounting. Its methods borrow the archive operation's transaction and cannot commit
/// separately. The payload remains provisional until complete collection finalization.
struct PageWrite {
    thread: i64,
    family: &'static str,
    sequence: i64,
    index: i64,
    payload: String,
}

impl PageWrite {
    /// Rejects superseded, missing, or completed generations before inspecting any page payload.
    async fn validate_generation(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<(), StoreError> {
        let current_sequence: Option<i64> = sqlx::query_scalar(
            "SELECT sequence FROM thread_family_reservations WHERE thread_id = ? AND family = ?",
        )
        .bind(self.thread)
        .bind(self.family)
        .fetch_optional(&mut *connection)
        .await?;
        if current_sequence != Some(self.sequence) {
            return Err(StoreError::StaleObservationGeneration);
        }
        let generation_status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM observation_generations WHERE thread_id = ? AND family = ? AND sequence = ?",
        )
        .bind(self.thread)
        .bind(self.family)
        .bind(self.sequence)
        .fetch_optional(&mut *connection)
        .await?;
        match generation_status.as_deref() {
            None => Err(StoreError::ObservationGenerationMissing),
            Some("complete") => Err(StoreError::StaleObservationGeneration),
            Some("reserved" | "incomplete") => Ok(()),
            Some(_) => Err(StoreError::ObservationGenerationMissing),
        }
    }

    /// Recognizes identical replay and rejects a different payload at the same page index.
    async fn is_replay(&self, connection: &mut SqliteConnection) -> Result<bool, StoreError> {
        let existing_page: Option<String> = sqlx::query_scalar(
            "SELECT payload_json FROM observation_staging_pages WHERE thread_id = ? AND family = ? AND sequence = ? AND page_index = ?",
        )
        .bind(self.thread)
        .bind(self.family)
        .bind(self.sequence)
        .bind(self.index)
        .fetch_optional(&mut *connection)
        .await?;
        if let Some(existing_page) = existing_page {
            if existing_page != self.payload {
                return Err(StoreError::StagedPageConflict);
            }
            return Ok(true);
        }

        Ok(false)
    }

    /// Inserts the provisional page and updates its generation count in the same transaction.
    async fn persist(&self, connection: &mut SqliteConnection) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO observation_staging_pages (thread_id, family, sequence, page_index, payload_json) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(self.thread)
        .bind(self.family)
        .bind(self.sequence)
        .bind(self.index)
        .bind(&self.payload)
        .execute(&mut *connection)
        .await?;
        let staged_pages =
            load_staged_pages(connection, self.thread, self.family, self.sequence).await?;
        let staged_count = count_staged_items(&staged_pages)?;
        sqlx::query(
            "UPDATE observation_generations SET status = 'reserved', received_items = ? WHERE thread_id = ? AND family = ? AND sequence = ?",
        )
        .bind(i64::try_from(staged_count).map_err(|_| StoreError::IntegerOutOfRange)?)
        .bind(self.thread)
        .bind(self.family)
        .bind(self.sequence)
        .execute(&mut *connection)
        .await?;
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

/// Counts received staged items, including repeated IDs across pages, before finalization.
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
