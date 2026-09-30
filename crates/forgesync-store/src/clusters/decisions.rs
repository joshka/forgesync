//! # Persist local cluster triage
//!
//! These `Archive` methods record dismissal, restoration, membership changes, and canonical-thread
//! choices. A decision changes the local triage view of an existing generation; it does not edit
//! GitHub or rewrite the source discussion.
//!
//! Decision events preserve the maintainer's action so later cluster reads can distinguish an
//! automatic proposal from an explicit choice. ID conversion and validation stay beside the write
//! path because malformed or out-of-range identifiers must fail before SQL receives them.
//!
//! Every successful decision commits state and its audit event in one fenced transaction. A
//! failed fence, unknown target, SQL write, or event insertion returns an error without committing
//! this decision. Repeated valid actions still append events; these APIs do not suppress history
//! merely because the visible state already matches the requested choice.
//!
//! Dismissal and exclusion reasons are limited to 2,048 UTF-8 bytes before trimming; persisted
//! reasons discard surrounding whitespace. Invalid reason length returns
//! `InvalidClusterGeneration`. Missing cluster IDs return `ClusterMissing`; removed,
//! foreign-repository, or unknown members return `ClusterMemberMissing`. A member must be active to
//! become canonical.
//!
//! Detail reads are observations, not write authority. These operations independently validate
//! writer fencing and target membership. Canonical selection changes the display choice while
//! preserving the generated representative, allowing future generations to retain their own policy.

use forgesync_core::identity::ThreadId;
use forgesync_core::timestamp::UtcTimestamp;
use sqlx::SqliteConnection;

use crate::archive::Archive;
use crate::clusters::generation_input::thread_row_id;
use crate::clusters::member_decision::{MemberDecision, MemberDecisionWrite};
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};

/// Explicit local dismissal choice, independent of active/retired generation lifecycle.
#[derive(Clone, Copy)]
enum ClusterDecision {
    /// Record a maintainer dismissal and its reason without removing generated membership.
    Dismiss,
    /// Clear dismissal and its stored reason without regenerating the cluster.
    Restore,
}

impl Archive {
    /// Dismisses a generated cluster as a local maintainer decision.
    ///
    /// Records the trimmed reason and action time with a `dismissed` audit event. Generated
    /// membership, lifecycle, and representative remain intact. An oversized reason is rejected
    /// before writable-archive and lease checks; state and event commit together.
    pub async fn dismiss_cluster_fenced(
        &self,
        token: &ArchiveLeaseToken,
        id: u64,
        reason: &str,
        at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        self.set_cluster_decision(token, id, ClusterDecision::Dismiss, reason, at)
            .await
    }

    /// Restores a locally dismissed generated cluster.
    ///
    /// Clears dismissal time and reason and appends a `restored` event. Restoration does not
    /// reactivate a retired generation or regenerate membership; those are separate operations.
    /// Repeating restoration still records the valid maintainer action.
    pub async fn restore_cluster_fenced(
        &self,
        token: &ArchiveLeaseToken,
        id: u64,
        at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        self.set_cluster_decision(token, id, ClusterDecision::Restore, "", at)
            .await
    }

    /// Records a local cluster decision without changing generated membership.
    async fn set_cluster_decision(
        &self,
        token: &ArchiveLeaseToken,
        id: u64,
        decision: ClusterDecision,
        reason: &str,
        at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        if reason.len() > 2048 {
            return Err(StoreError::InvalidClusterGeneration);
        }
        let cluster_id = checked_cluster_id(id)?;
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let event = match decision {
            ClusterDecision::Dismiss => "dismissed",
            ClusterDecision::Restore => "restored",
        };
        let result = if matches!(decision, ClusterDecision::Dismiss) {
            sqlx::query("UPDATE clusters SET dismissed_at_us = ?, dismissal_reason = ?, updated_at_us = ? WHERE id = ?")
                .bind(at.unix_microseconds())
                .bind(reason.trim())
                .bind(at.unix_microseconds())
                .bind(cluster_id)
                .execute(&mut *transaction)
                .await?
        } else {
            sqlx::query("UPDATE clusters SET dismissed_at_us = NULL, dismissal_reason = '', updated_at_us = ? WHERE id = ?")
                .bind(at.unix_microseconds())
                .bind(cluster_id)
                .execute(&mut *transaction)
                .await?
        };
        if result.rows_affected() != 1 {
            return Err(StoreError::ClusterMissing);
        }
        insert_cluster_event(
            &mut transaction,
            cluster_id,
            None,
            event,
            None,
            reason.trim(),
            at,
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Excludes one current generated member as a local decision.
    ///
    /// Persists exclusion across future generations and marks current membership excluded. If
    /// this member is canonical, clears that choice in the same transaction. The trimmed reason
    /// and `member_excluded` event are committed with the state changes; source evidence is
    /// retained.
    pub async fn exclude_cluster_member_fenced(
        &self,
        token: &ArchiveLeaseToken,
        id: u64,
        thread: &ThreadId,
        reason: &str,
        at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        self.set_member_decision(token, id, thread, MemberDecision::Exclude, reason, at)
            .await
    }

    /// Includes one previously excluded generated member.
    ///
    /// Persists inclusion and restores active membership with a `member_included` audit event.
    /// Inclusion does not select this member as canonical or restore a canonical choice cleared
    /// by exclusion. Removed or unknown members cannot be restored through this operation.
    pub async fn include_cluster_member_fenced(
        &self,
        token: &ArchiveLeaseToken,
        id: u64,
        thread: &ThreadId,
        at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        self.set_member_decision(token, id, thread, MemberDecision::Include, "", at)
            .await
    }

    /// Records an include or exclude decision for one current member.
    async fn set_member_decision(
        &self,
        token: &ArchiveLeaseToken,
        id: u64,
        thread: &ThreadId,
        decision: MemberDecision,
        reason: &str,
        at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        if reason.len() > 2048 {
            return Err(StoreError::InvalidClusterGeneration);
        }
        let cluster_id = checked_cluster_id(id)?;
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let member_id = current_cluster_member_id(&mut transaction, cluster_id, thread).await?;
        let write = MemberDecisionWrite {
            cluster_id,
            member_id,
            decision,
            reason,
            at,
        };
        write.apply(&mut transaction).await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Sets the canonical member while preserving the generated representative for future runs.
    ///
    /// Requires current active membership in the selected cluster. The canonical choice and
    /// `canonical_set` event commit together; excluded/removed members return
    /// `ClusterMemberMissing`. A later generation or local exclusion may invalidate the choice,
    /// so prior inspection does not bypass membership or fencing validation here.
    pub async fn set_cluster_canonical_fenced(
        &self,
        token: &ArchiveLeaseToken,
        id: u64,
        thread: &ThreadId,
        at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        let cluster_id = checked_cluster_id(id)?;
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let member_id = current_cluster_member_id(&mut transaction, cluster_id, thread).await?;
        let state: String = sqlx::query_scalar(
            "SELECT state FROM cluster_memberships WHERE cluster_id = ? AND thread_id = ?",
        )
        .bind(cluster_id)
        .bind(member_id)
        .fetch_one(&mut *transaction)
        .await?;
        if state != "active" {
            return Err(StoreError::ClusterMemberMissing);
        }
        let result = sqlx::query(
            "UPDATE clusters SET canonical_thread_id = ?, updated_at_us = ? WHERE id = ?",
        )
        .bind(member_id)
        .bind(at.unix_microseconds())
        .bind(cluster_id)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(StoreError::ClusterMissing);
        }
        insert_cluster_event(
            &mut transaction,
            cluster_id,
            None,
            "canonical_set",
            Some(member_id),
            "",
            at,
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }
}

/// Appends a durable audit event for a local maintainer action.
pub async fn insert_cluster_event(
    connection: &mut SqliteConnection,
    cluster_id: i64,
    run_id: Option<i64>,
    event_type: &str,
    thread_id: Option<i64>,
    reason: &str,
    at: UtcTimestamp,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO cluster_events (cluster_id, run_id, event_type, thread_id, reason, created_at_us) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(cluster_id)
    .bind(run_id)
    .bind(event_type)
    .bind(thread_id)
    .bind(reason)
    .bind(at.unix_microseconds())
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// Rejects a decision targeting a removed or unknown cluster member.
async fn current_cluster_member_id(
    connection: &mut SqliteConnection,
    cluster_id: i64,
    thread: &ThreadId,
) -> Result<i64, StoreError> {
    let cluster_repository: Option<(String, String)> = sqlx::query_as(
        "SELECT r.host, r.provider_id FROM clusters c JOIN repositories r ON r.id = c.repository_id WHERE c.id = ?",
    )
    .bind(cluster_id)
    .fetch_optional(&mut *connection)
    .await?;
    let (host, repository_provider_id) = cluster_repository.ok_or(StoreError::ClusterMissing)?;
    if host != thread.repository().host().as_str()
        || repository_provider_id != thread.repository().provider_id().as_str()
    {
        return Err(StoreError::ClusterMemberMissing);
    }
    let repository_id: i64 = sqlx::query_scalar("SELECT repository_id FROM clusters WHERE id = ?")
        .bind(cluster_id)
        .fetch_one(&mut *connection)
        .await?;
    let thread_id = thread_row_id(connection, repository_id, thread).await?;
    let membership: Option<String> = sqlx::query_scalar(
        "SELECT state FROM cluster_memberships WHERE cluster_id = ? AND thread_id = ?",
    )
    .bind(cluster_id)
    .bind(thread_id)
    .fetch_optional(&mut *connection)
    .await?;
    if membership.is_none_or(|state| state == "removed") {
        return Err(StoreError::ClusterMemberMissing);
    }
    Ok(thread_id)
}

/// Checks an archive-local cluster ID before binding it to SQLite.
pub fn checked_cluster_id(id: u64) -> Result<i64, StoreError> {
    i64::try_from(id)
        .ok()
        .filter(|id| *id > 0)
        .ok_or(StoreError::ClusterMissing)
}
