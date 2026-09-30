//! # Durable positions for resumable acquisition
//!
//! Checkpoint methods on `Archive` retain the position reached by provider enumeration. The engine
//! reads these positions before resuming work and advances them only after the related archive
//! updates have succeeded. This keeps a failed refresh from being mistaken for completed coverage.
//!
//! These methods store progress, not the discussion content itself. Thread observations and
//! child-family staging live in their own modules; a checkpoint tells the next run where to begin
//! looking again.
//!
//! [`Archive::closed_sweep_watermark`] reads the last committed source-time boundary. Absence is
//! different from a zero timestamp: an unregistered repository or a repository without this
//! checkpoint both return `None`. The engine decides the overlap and source query for its next
//! sweep; the store does not interpret provider clocks as acquisition order.
//!
//! [`Archive::commit_closed_sweep_watermark`] is the publication boundary. Within one transaction
//! it checks the active writer lease, resolves the registered repository, verifies a completed
//! scan with no continuation at the supplied acquisition sequence, and advances the checkpoint
//! only when that sequence is newer. A failed check rolls back the checkpoint update. Source-time
//! watermark values are retained as supplied; sequence ordering, not timestamp magnitude, fences
//! an older acquisition from overwriting newer progress.

use forgesync_core::identity::{ObservationSequence, RepositoryId};
use forgesync_core::timestamp::UtcTimestamp;
use sqlx::SqliteConnection;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};

impl Archive {
    /// Returns the committed closed-sweep source-time boundary, if one exists.
    ///
    /// Returns `None` for both an unregistered repository and a registered repository without a
    /// closed-sweep checkpoint. This read does not claim a lease or begin another scan. A later
    /// writer may advance the checkpoint after the query.
    ///
    /// # Errors
    ///
    /// Returns database errors when the lookup fails and [`StoreError::InvalidCreatedAt`] when
    /// the stored microsecond value cannot be represented as a domain timestamp.
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
        value
            .map(|value| {
                UtcTimestamp::from_unix_microseconds(value).map_err(StoreError::InvalidCreatedAt)
            })
            .transpose()
    }

    /// Publishes a closed-sweep boundary after the same-sequence scan is complete.
    ///
    /// `sequence` identifies the durable scan, `watermark` is the provider source-time boundary
    /// selected by the workflow, and `updated_at` records when the checkpoint was published. The
    /// store does not derive or compare these two timestamps. The scan must have status `complete`
    /// and no next-page URL; this check does not independently prove which source query was used.
    ///
    /// The active lease check, scan check, and checkpoint upsert share one transaction. Replaying
    /// an equal sequence is rejected rather than treated as an idempotent success. Any error
    /// leaves the previous checkpoint intact.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::ReadOnlyArchive`] without a writer pool, lease errors when the token
    /// no longer authorizes writing, [`StoreError::RepositoryMissing`] for an unknown repository,
    /// and [`StoreError::IntegerOutOfRange`] for a sequence outside SQLite's signed range. An
    /// unfinished or absent scan returns [`StoreError::InvalidRepositoryThreadScan`]; an equal or
    /// older checkpoint sequence returns [`StoreError::StaleRepositoryThreadScan`]. Database
    /// failures are propagated.
    pub async fn commit_closed_sweep_watermark(
        &self,
        token: &ArchiveLeaseToken,
        repository: &RepositoryId,
        sequence: ObservationSequence,
        watermark: UtcTimestamp,
        updated_at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let sequence = i64::try_from(sequence.get()).map_err(|_| StoreError::IntegerOutOfRange)?;
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

/// Resolves a registered repository to its SQLite key before checkpoint mutation.
async fn repository_row_id(
    connection: &mut SqliteConnection,
    repository: &RepositoryId,
) -> Result<i64, StoreError> {
    sqlx::query_scalar("SELECT id FROM repositories WHERE host = ? AND provider_id = ?")
        .bind(repository.host().as_str())
        .bind(repository.provider_id().as_str())
        .fetch_optional(&mut *connection)
        .await?
        .ok_or(StoreError::RepositoryMissing)
}
