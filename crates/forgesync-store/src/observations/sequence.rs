//! Archive-wide acquisition sequence allocation.
//!
//! Sequences order local acquisition independently of provider timestamps. Allocation commits
//! before returning, so gaps from failed acquisitions are valid and need no repair.

use forgesync_core::identity::ObservationSequence;
use forgesync_core::timestamp::UtcTimestamp;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::sql::sequence_from_sql;

impl Archive {
    /// Commits the next acquisition sequence without a lease check.
    ///
    /// `started_at` is diagnostic metadata, not ordering input.
    pub async fn reserve_observation_sequence(
        &self,
        started_at: UtcTimestamp,
    ) -> Result<ObservationSequence, StoreError> {
        self.reserve_sequence(started_at, None).await
    }

    /// Commits the next acquisition sequence while the supplied writer lease is active.
    pub async fn reserve_observation_sequence_fenced(
        &self,
        started_at: UtcTimestamp,
        token: &ArchiveLeaseToken,
    ) -> Result<ObservationSequence, StoreError> {
        self.reserve_sequence(started_at, Some(token)).await
    }

    /// Increments the shared counter, optionally checking the lease in the same transaction.
    ///
    /// Both public forms stay because the engine's sync module calls the fenced signature.
    async fn reserve_sequence(
        &self,
        started_at: UtcTimestamp,
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<ObservationSequence, StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        if let Some(token) = token {
            require_active_archive_lease(&mut transaction, token).await?;
        }
        let value: i64 = sqlx::query_scalar(
            "UPDATE observation_sequence SET value = value + 1, last_started_at_us = ? WHERE singleton = 1 RETURNING value",
        )
        .bind(started_at.unix_microseconds())
        .fetch_one(&mut *transaction)
        .await?;
        let sequence = sequence_from_sql(value)?;
        transaction.commit().await?;
        Ok(sequence)
    }
}
