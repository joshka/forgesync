//! # Select an active member as the local canonical discussion
//!
//! `CanonicalSelection` binds a checked cluster and resolved current member to one action time.
//! The archive owns transaction creation, writer fencing, and member resolution before preparing
//! this selection. `apply` additionally requires active membership; an excluded member cannot
//! become canonical even though it remains visible in cluster detail.
//!
//! Canonical update and its audit event run in the same caller-owned transaction. The generated
//! representative is retained, so local display preference and later graph proposals stay distinct.
//! No source evidence or GitHub state changes. Later exclusion or generation can clear this choice.
//!
//! A failed membership check, missing cluster, or audit write prevents the caller from committing
//! this decision. This projection never commits or takes over a lease independently.

use forgesync_core::timestamp::UtcTimestamp;
use sqlx::SqliteConnection;

use crate::clusters::decisions::insert_cluster_event;
use crate::error::StoreError;

/// Prepared canonical decision within a fenced archive transaction.
pub struct CanonicalSelection {
    /// Checked cluster row selected by the maintainer.
    pub cluster_id: i64,
    /// Current member resolved inside this transaction.
    pub member_id: i64,
    /// Action time shared by canonical state and audit evidence.
    pub at: UtcTimestamp,
}

impl CanonicalSelection {
    /// Requires active membership, updates the canonical row, and records the decision together.
    pub async fn apply(&self, connection: &mut SqliteConnection) -> Result<(), StoreError> {
        self.require_active(connection).await?;
        self.select(connection).await?;
        self.audit(connection).await
    }

    /// Rejects an excluded member even though its current generated membership still exists.
    async fn require_active(&self, connection: &mut SqliteConnection) -> Result<(), StoreError> {
        let state: String = sqlx::query_scalar(
            "SELECT state FROM cluster_memberships WHERE cluster_id = ? AND thread_id = ?",
        )
        .bind(self.cluster_id)
        .bind(self.member_id)
        .fetch_one(connection)
        .await?;
        if state != "active" {
            return Err(StoreError::ClusterMemberMissing);
        }
        Ok(())
    }

    /// Updates local canonical preference while retaining the generated representative.
    async fn select(&self, connection: &mut SqliteConnection) -> Result<(), StoreError> {
        let result = sqlx::query(
            "UPDATE clusters SET canonical_thread_id = ?, updated_at_us = ? WHERE id = ?",
        )
        .bind(self.member_id)
        .bind(self.at.unix_microseconds())
        .bind(self.cluster_id)
        .execute(connection)
        .await?;
        if result.rows_affected() != 1 {
            return Err(StoreError::ClusterMissing);
        }
        Ok(())
    }

    /// Appends canonical-selection evidence within the same transaction as the state update.
    async fn audit(&self, connection: &mut SqliteConnection) -> Result<(), StoreError> {
        insert_cluster_event(
            connection,
            self.cluster_id,
            None,
            "canonical_set",
            Some(self.member_id),
            "",
            self.at,
        )
        .await
    }
}
