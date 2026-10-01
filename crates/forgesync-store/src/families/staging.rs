//! Provisional child-family pages; they never become canonical membership until a complete finish
//! validates the whole page set.

use serde::Serialize;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::families::ChildFamilyPage;
use crate::families::query::require_child_family;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::sql::{family_name, thread_row_id, to_sql_sequence};

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
        require_child_family(family)?;
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let family = family_name(family);
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
