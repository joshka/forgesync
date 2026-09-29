use forgesync_core::identity::{ObservationSequence, RepositoryId};
use forgesync_core::timestamp::UtcTimestamp;
use sqlx::SqliteConnection;

use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::{Archive, StoreError};

impl Archive {
    /// Returns the last successfully completed closed-thread sweep watermark.
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

    /// Advances a closed-sweep watermark only after its same-sequence scan completed.
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
