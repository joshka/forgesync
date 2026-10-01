//! Provisional child-family pages and the page-set checks used when finishing a collection.
//!
//! Staged pages never become canonical membership until a complete finish validates the whole set.

use std::collections::BTreeMap;

use serde::Serialize;
use sqlx::{Row, SqliteConnection};

use crate::archive::Archive;
use crate::error::StoreError;
use crate::families::{ChildFamilyPage, StagedPage};
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::observation_sql::{
    evidence_family_name, is_child_family, thread_row_id, to_sql_sequence,
};
use crate::observations::StagedItem;

impl Archive {
    /// Persists a provisional page under the archive writer fence without changing canonical
    /// membership.
    ///
    /// Replaying a page index succeeds only with the identical serialized payload. A completed,
    /// missing, or superseded generation rejects the page.
    pub async fn stage_child_family_page_fenced<T>(
        &self,
        page: ChildFamilyPage<'_, T>,
        token: &ArchiveLeaseToken,
    ) -> Result<(), StoreError>
    where
        T: Serialize,
    {
        let ChildFamilyPage {
            thread,
            family,
            sequence,
            page_index,
            items,
        } = page;
        if !is_child_family(family) {
            return Err(StoreError::UnsupportedObservationFamily(
                evidence_family_name(family).to_owned(),
            ));
        }
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let family = evidence_family_name(family);
        let page_index = i64::from(page_index);
        let sequence = to_sql_sequence(sequence)?;
        let payload = serde_json::to_string(items)?;
        let item_count = i64::try_from(items.len()).map_err(|_| StoreError::IntegerOutOfRange)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let thread = thread_row_id(&mut transaction, thread).await?;

        let current_sequence: Option<i64> = sqlx::query_scalar(
            "SELECT sequence FROM thread_family_reservations WHERE thread_id = ? AND family = ?",
        )
        .bind(thread)
        .bind(family)
        .fetch_optional(&mut *transaction)
        .await?;
        if current_sequence != Some(sequence) {
            return Err(StoreError::StaleObservationGeneration);
        }
        let generation_status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM observation_generations WHERE thread_id = ? AND family = ? AND sequence = ?",
        )
        .bind(thread)
        .bind(family)
        .bind(sequence)
        .fetch_optional(&mut *transaction)
        .await?;
        match generation_status.as_deref() {
            Some("reserved" | "incomplete") => {}
            Some("complete") => return Err(StoreError::StaleObservationGeneration),
            _ => return Err(StoreError::ObservationGenerationMissing),
        }

        // Replay compares exact serialized text rather than semantic JSON equality.
        let existing_page: Option<String> = sqlx::query_scalar(
            "SELECT payload_json FROM observation_staging_pages WHERE thread_id = ? AND family = ? AND sequence = ? AND page_index = ?",
        )
        .bind(thread)
        .bind(family)
        .bind(sequence)
        .bind(page_index)
        .fetch_optional(&mut *transaction)
        .await?;
        match existing_page {
            Some(existing) if existing != payload => return Err(StoreError::StagedPageConflict),
            Some(_) => {}
            None => {
                sqlx::query(
                    "INSERT INTO observation_staging_pages (thread_id, family, sequence, page_index, payload_json) VALUES (?, ?, ?, ?, ?)",
                )
                .bind(thread)
                .bind(family)
                .bind(sequence)
                .bind(page_index)
                .bind(&payload)
                .execute(&mut *transaction)
                .await?;
                sqlx::query(
                    "UPDATE observation_generations SET status = 'reserved', received_items = received_items + ? WHERE thread_id = ? AND family = ? AND sequence = ?",
                )
                .bind(item_count)
                .bind(thread)
                .bind(family)
                .bind(sequence)
                .execute(&mut *transaction)
                .await?;
            }
        }
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
