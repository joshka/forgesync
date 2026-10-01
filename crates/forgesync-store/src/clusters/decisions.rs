//! Local maintainer decisions on generated clusters.
//!
//! Each decision commits its state change and audit event in one fenced transaction. Repeated
//! valid actions still append events. Decisions never edit GitHub or source discussions, and
//! canonical selection keeps the generated representative for future generations.

use forgesync_core::identity::ThreadId;
use forgesync_core::timestamp::UtcTimestamp;
use sqlx::SqliteConnection;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::sql::to_sql_integer;

/// Longest accepted dismissal or exclusion reason, in UTF-8 bytes before trimming.
const MAX_REASON_BYTES: usize = 2048;

impl Archive {
    /// Dismisses a generated cluster with a trimmed reason, keeping its membership intact.
    pub async fn dismiss_cluster_fenced(
        &self,
        token: &ArchiveLeaseToken,
        id: u64,
        reason: &str,
        at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        let reason = checked_reason(reason)?;
        let cluster_id = checked_cluster_id(id)?;
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let result = sqlx::query(
            "UPDATE clusters SET dismissed_at_us = ?, dismissal_reason = ?, updated_at_us = ? WHERE id = ?",
        )
        .bind(at.unix_microseconds())
        .bind(reason)
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
            "dismissed",
            None,
            reason,
            at,
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Clears a local dismissal; it does not reactivate a retired generation.
    pub async fn restore_cluster_fenced(
        &self,
        token: &ArchiveLeaseToken,
        id: u64,
        at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        let cluster_id = checked_cluster_id(id)?;
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let result = sqlx::query(
            "UPDATE clusters SET dismissed_at_us = NULL, dismissal_reason = '', updated_at_us = ? WHERE id = ?",
        )
        .bind(at.unix_microseconds())
        .bind(cluster_id)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(StoreError::ClusterMissing);
        }
        insert_cluster_event(&mut transaction, cluster_id, None, "restored", None, "", at).await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Excludes a current member across future generations, clearing it if it was canonical.
    pub async fn exclude_cluster_member_fenced(
        &self,
        token: &ArchiveLeaseToken,
        id: u64,
        thread: &ThreadId,
        reason: &str,
        at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        self.set_member_excluded(token, id, thread, true, reason, at)
            .await
    }

    /// Restores a previously excluded member without making it canonical.
    pub async fn include_cluster_member_fenced(
        &self,
        token: &ArchiveLeaseToken,
        id: u64,
        thread: &ThreadId,
        at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        self.set_member_excluded(token, id, thread, false, "", at)
            .await
    }

    /// Records a durable include/exclude decision, updates current membership state, and audits it.
    async fn set_member_excluded(
        &self,
        token: &ArchiveLeaseToken,
        id: u64,
        thread: &ThreadId,
        excluded: bool,
        reason: &str,
        at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        let reason = checked_reason(reason)?;
        let cluster_id = checked_cluster_id(id)?;
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let (member_id, _) = current_cluster_member(&mut transaction, cluster_id, thread).await?;
        sqlx::query(
            "INSERT INTO cluster_member_decisions (cluster_id, thread_id, excluded, reason, updated_at_us) VALUES (?, ?, ?, ?, ?) ON CONFLICT (cluster_id, thread_id) DO UPDATE SET excluded = excluded.excluded, reason = excluded.reason, updated_at_us = excluded.updated_at_us",
        )
        .bind(cluster_id)
        .bind(member_id)
        .bind(i64::from(excluded))
        .bind(reason)
        .bind(at.unix_microseconds())
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE cluster_memberships SET state = ?, updated_at_us = ? WHERE cluster_id = ? AND thread_id = ?",
        )
        .bind(if excluded { "excluded" } else { "active" })
        .bind(at.unix_microseconds())
        .bind(cluster_id)
        .bind(member_id)
        .execute(&mut *transaction)
        .await?;
        if excluded {
            sqlx::query(
                "UPDATE clusters SET canonical_thread_id = NULL, updated_at_us = ? WHERE id = ? AND canonical_thread_id = ?",
            )
            .bind(at.unix_microseconds())
            .bind(cluster_id)
            .bind(member_id)
            .execute(&mut *transaction)
            .await?;
        }
        let event = if excluded {
            "member_excluded"
        } else {
            "member_included"
        };
        insert_cluster_event(
            &mut transaction,
            cluster_id,
            None,
            event,
            Some(member_id),
            reason,
            at,
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Sets an active member as the local canonical discussion, keeping the generated
    /// representative.
    ///
    /// Excluded or removed members return [`StoreError::ClusterMemberMissing`].
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
        let (member_id, state) =
            current_cluster_member(&mut transaction, cluster_id, thread).await?;
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

/// Appends a cluster audit event inside the caller's transaction.
///
/// `run_id` ties generated events to a build; `thread_id` identifies member-specific actions.
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

/// Resolves a non-removed member's thread row and membership state in one query.
///
/// Returns `ClusterMissing` for an unknown cluster, `ThreadMissing` for an unknown thread in the
/// cluster's repository, and `ClusterMemberMissing` for a foreign, removed, or non-member thread.
async fn current_cluster_member(
    connection: &mut SqliteConnection,
    cluster_id: i64,
    thread: &ThreadId,
) -> Result<(i64, String), StoreError> {
    let row: Option<(bool, Option<i64>, Option<String>)> = sqlx::query_as(
        "SELECT r.host = ? AND r.provider_id = ?, t.id, m.state FROM clusters c JOIN repositories r ON r.id = c.repository_id LEFT JOIN threads t ON t.repository_id = c.repository_id AND t.provider_id = ? AND t.number = ? LEFT JOIN cluster_memberships m ON m.cluster_id = c.id AND m.thread_id = t.id WHERE c.id = ?",
    )
    .bind(thread.repository().host().as_str())
    .bind(thread.repository().provider_id().as_str())
    .bind(thread.provider_id().as_str())
    .bind(to_sql_integer(thread.number().get())?)
    .bind(cluster_id)
    .fetch_optional(&mut *connection)
    .await?;
    match row {
        None => Err(StoreError::ClusterMissing),
        Some((false, _, _)) => Err(StoreError::ClusterMemberMissing),
        Some((true, None, _)) => Err(StoreError::ThreadMissing),
        Some((true, Some(thread_id), Some(state))) if state != "removed" => Ok((thread_id, state)),
        Some((true, Some(_), _)) => Err(StoreError::ClusterMemberMissing),
    }
}

/// Validates reason length and returns the trimmed reason that is persisted.
fn checked_reason(reason: &str) -> Result<&str, StoreError> {
    if reason.len() > MAX_REASON_BYTES {
        return Err(StoreError::InvalidClusterGeneration);
    }
    Ok(reason.trim())
}

/// Checks a public cluster ID before binding it to SQLite.
pub fn checked_cluster_id(id: u64) -> Result<i64, StoreError> {
    i64::try_from(id)
        .ok()
        .filter(|id| *id > 0)
        .ok_or(StoreError::ClusterMissing)
}
