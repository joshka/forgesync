//! # Persist local cluster dismissal or restoration
//!
//! `ClusterDecisionWrite` binds a checked cluster row, typed dismissal choice, validated reason,
//! and action time. The archive opens/fences the transaction before preparing this write and
//! commits only after state mutation and audit insertion succeed.
//!
//! Dismissal stores a trimmed reason and timestamp; restoration clears both. Neither changes
//! generated membership, lifecycle, or representative. Repeated valid actions still add audit
//! events, allowing inspection to distinguish a maintainer choice from generation policy.
//!
//! The state update must affect exactly one cluster. A missing row or event-write failure leaves
//! the caller's transaction uncommitted. This module performs no independent commit or provider
//! I/O.

use forgesync_core::timestamp::UtcTimestamp;
use sqlx::SqliteConnection;

use crate::clusters::decisions::insert_cluster_event;
use crate::error::StoreError;

/// Explicit local dismissal choice, independent of active/retired generation lifecycle.
#[derive(Clone, Copy)]
pub enum ClusterDecision {
    /// Record a maintainer dismissal and its reason without removing generated membership.
    Dismiss,
    /// Clear dismissal and its stored reason without regenerating the cluster.
    Restore,
}

/// Prepared local cluster decision whose state and audit share one action context.
pub struct ClusterDecisionWrite<'a> {
    /// Checked archive cluster row selected by the maintainer.
    pub cluster_id: i64,
    /// Explicit dismissal or restoration policy.
    pub decision: ClusterDecision,
    /// Validated reason; restoration supplies an empty value.
    pub reason: &'a str,
    /// Action time shared by state and audit evidence.
    pub at: UtcTimestamp,
}

impl ClusterDecisionWrite<'_> {
    /// Updates local state and appends its event inside the caller's fenced transaction.
    pub async fn apply(&self, connection: &mut SqliteConnection) -> Result<(), StoreError> {
        let affected = match self.decision {
            ClusterDecision::Dismiss => self.dismiss(connection).await?,
            ClusterDecision::Restore => self.restore(connection).await?,
        };
        if affected != 1 {
            return Err(StoreError::ClusterMissing);
        }
        self.audit(connection).await
    }

    /// Records dismissal time and trimmed reason without changing generated membership.
    async fn dismiss(&self, connection: &mut SqliteConnection) -> Result<u64, StoreError> {
        let query = sqlx::query(
            "UPDATE clusters SET dismissed_at_us = ?, dismissal_reason = ?, updated_at_us = ? WHERE id = ?",
        );
        let result = query
            .bind(self.at.unix_microseconds())
            .bind(self.reason.trim())
            .bind(self.at.unix_microseconds())
            .bind(self.cluster_id)
            .execute(connection)
            .await?;
        Ok(result.rows_affected())
    }

    /// Clears local dismissal state without reactivating a retired generation.
    async fn restore(&self, connection: &mut SqliteConnection) -> Result<u64, StoreError> {
        let query = sqlx::query(
            "UPDATE clusters SET dismissed_at_us = NULL, dismissal_reason = '', updated_at_us = ? WHERE id = ?",
        );
        let result = query
            .bind(self.at.unix_microseconds())
            .bind(self.cluster_id)
            .execute(connection)
            .await?;
        Ok(result.rows_affected())
    }

    /// Records the corresponding action with the same normalized reason and time.
    async fn audit(&self, connection: &mut SqliteConnection) -> Result<(), StoreError> {
        insert_cluster_event(
            connection,
            self.cluster_id,
            None,
            self.decision.event(),
            None,
            self.reason.trim(),
            self.at,
        )
        .await
    }
}

impl ClusterDecision {
    /// Supplies the stable event label paired with the local state mutation.
    fn event(self) -> &'static str {
        match self {
            Self::Dismiss => "dismissed",
            Self::Restore => "restored",
        }
    }
}
