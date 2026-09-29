//! Observation sequence operations.

use super::{
    Archive, ArchiveLeaseToken, ObservationSequence, StoreError, UtcTimestamp, checked_sequence,
    require_active_archive_lease,
};

impl Archive {
    /// Reserves and durably increments the archive-wide acquisition sequence.
    pub async fn reserve_observation_sequence(
        &self,
        started_at: UtcTimestamp,
    ) -> Result<ObservationSequence, StoreError> {
        self.reserve_observation_sequence_inner(started_at, None)
            .await
    }

    /// Reserves a new observation sequence while the supplied archive lease remains current.
    pub async fn reserve_observation_sequence_fenced(
        &self,
        started_at: UtcTimestamp,
        token: &ArchiveLeaseToken,
    ) -> Result<ObservationSequence, StoreError> {
        self.reserve_observation_sequence_inner(started_at, Some(token))
            .await
    }

    async fn reserve_observation_sequence_inner(
        &self,
        started_at: UtcTimestamp,
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<ObservationSequence, StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        if let Some(token) = token {
            require_active_archive_lease(&mut transaction, token).await?;
        }
        let raw_sequence: i64 = sqlx::query_scalar(
            "UPDATE observation_sequence SET value = value + 1, last_started_at_us = ? WHERE singleton = 1 RETURNING value",
        )
        .bind(started_at.unix_microseconds())
        .fetch_one(&mut *transaction)
        .await?;
        let sequence = checked_sequence(raw_sequence)?;
        transaction.commit().await?;
        Ok(sequence)
    }
}
