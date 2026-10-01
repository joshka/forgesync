//! Closed-sweep checkpoints that let the next sync resume from a committed source-time boundary.
//!
//! Acquisition sequence, not watermark magnitude, fences an older scan from overwriting newer
//! progress; the store never interprets provider clocks as acquisition order.

use forgesync_core::identity::{ObservationSequence, RepositoryId};
use forgesync_core::timestamp::UtcTimestamp;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::sql::{repository_row_id, timestamp_from_sql, to_sql_sequence};

/// A closed-sweep source boundary tied to the scan that permits its publication.
#[derive(Clone, Copy, Debug)]
pub struct ClosedSweepCheckpoint<'a> {
    /// Registered repository whose closed-sweep progress is being published.
    pub repository: &'a RepositoryId,
    /// Archive-reserved scan sequence used to reject older or equal checkpoint updates.
    pub sequence: ObservationSequence,
    /// Provider source-time boundary selected by the workflow, not derived by the store.
    pub watermark: UtcTimestamp,
    /// Local publication time for diagnostics, independent of the source boundary.
    pub updated_at: UtcTimestamp,
}

impl Archive {
    /// Returns the committed closed-sweep source-time boundary, if one exists.
    ///
    /// Returns `None` for an unregistered repository as well as one without a checkpoint.
    pub async fn closed_sweep_watermark(
        &self,
        repository: &RepositoryId,
    ) -> Result<Option<UtcTimestamp>, StoreError> {
        let value: Option<i64> = sqlx::query_scalar(
            "SELECT watermark_us FROM repository_checkpoints WHERE repository_id = (SELECT id FROM repositories WHERE host = ? AND provider_id = ?) AND checkpoint = 'closed_sweep'",
        )
        .bind(repository.host().as_str())
        .bind(repository.provider_id().as_str())
        .fetch_optional(&self.reader)
        .await?;
        value.map(timestamp_from_sql).transpose()
    }

    /// Publishes a closed-sweep boundary after the same-sequence scan is complete.
    ///
    /// The scan must be complete with no next-page cursor, and the sequence must be newer than the
    /// stored checkpoint; replaying an equal sequence is rejected as stale.
    pub async fn commit_closed_sweep_watermark(
        &self,
        token: &ArchiveLeaseToken,
        checkpoint: ClosedSweepCheckpoint<'_>,
    ) -> Result<(), StoreError> {
        let ClosedSweepCheckpoint {
            repository,
            sequence,
            watermark,
            updated_at,
        } = checkpoint;
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let sequence = to_sql_sequence(sequence)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let repository_row_id = repository_row_id(&mut transaction, repository).await?;
        let scan_complete: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM repository_thread_scans WHERE repository_id = ? AND sequence = ? AND status = 'complete' AND next_page_url IS NULL)",
        )
        .bind(repository_row_id)
        .bind(sequence)
        .fetch_one(&mut *transaction)
        .await?;
        if scan_complete != 1 {
            return Err(StoreError::InvalidRepositoryThreadScan);
        }
        let result = sqlx::query(
            "INSERT INTO repository_checkpoints (repository_id, checkpoint, watermark_us, sequence, updated_at_us) VALUES (?, 'closed_sweep', ?, ?, ?) ON CONFLICT (repository_id, checkpoint) DO UPDATE SET watermark_us = excluded.watermark_us, sequence = excluded.sequence, updated_at_us = excluded.updated_at_us WHERE excluded.sequence > repository_checkpoints.sequence",
        )
        .bind(repository_row_id)
        .bind(watermark.unix_microseconds())
        .bind(sequence)
        .bind(updated_at.unix_microseconds())
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(StoreError::StaleRepositoryThreadScan);
        }
        transaction.commit().await?;
        Ok(())
    }
}
