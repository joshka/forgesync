//! # Allocate and convert acquisition sequences
//!
//! Observation sequences order local acquisition events independently of provider timestamps.
//! These archive methods allocate the next durable value and check the domain sequence before
//! committing its SQLite representation.
//!
//! Reserve a sequence before child-family provider I/O when the final result may arrive later. The
//! ordering policy itself lives in `ordering`; this module ensures all callers persist a
//! consistent, bounded sequence value.
//!
//! The singleton counter is shared across repositories and resource families. Allocation commits
//! before returning, so another reservation receives a later value even if the earlier acquisition
//! later fails or is cancelled. Gaps are valid; a sequence is neither a count of successful pages
//! nor proof that any provider evidence has been collected.
//!
//! `started_at` records the caller's acquisition start clock for diagnostics. It does not select
//! the next counter value or establish provider freshness. The SQL increment orders allocation;
//! later observation application combines that order with source-clock and completeness policy.
//!
//! The fenced method checks active lease ownership in the counter transaction. The unfenced
//! method provides explicit local allocation without that workflow guard. Neither method reserves
//! a child-family generation, performs network I/O, or starts a run: those are distinct operations
//! at their owning boundaries.

use forgesync_core::identity::ObservationSequence;
use forgesync_core::timestamp::UtcTimestamp;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::observations::checked_sequence;

impl Archive {
    /// Commits the next archive-wide acquisition value without a workflow lease check.
    ///
    /// The supplied start timestamp is diagnostic metadata, not ordering input. A successful
    /// reservation remains consumed if later acquisition fails; gaps require no repair. Use
    /// [`Self::reserve_observation_sequence_fenced`] for coordinated workflow writes.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::ReadOnlyArchive`] without a writer and propagates database or checked
    /// stored-sequence failures. No value is returned until the counter transaction commits.
    pub async fn reserve_observation_sequence(
        &self,
        started_at: UtcTimestamp,
    ) -> Result<ObservationSequence, StoreError> {
        self.reserve_observation_sequence_inner(started_at, None)
            .await
    }

    /// Commits the next acquisition value while the supplied writer lease is active.
    ///
    /// Lease validation, increment, and checked sequence conversion share one transaction. The
    /// returned sequence grants no continuing lease authority: later mutations recheck their token.
    /// This allocates only the counter, not a collection generation or complete observation.
    ///
    /// # Errors
    ///
    /// Returns read-only, lease/clock, database, or invalid stored-sequence errors. A stale token
    /// rejects allocation before the counter update.
    pub async fn reserve_observation_sequence_fenced(
        &self,
        started_at: UtcTimestamp,
        token: &ArchiveLeaseToken,
    ) -> Result<ObservationSequence, StoreError> {
        self.reserve_observation_sequence_inner(started_at, Some(token))
            .await
    }

    /// Allocates and checks the next sequence in a transaction, optionally validating its lease.
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
