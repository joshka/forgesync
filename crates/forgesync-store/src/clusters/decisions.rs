//! Decisions for durable clusters.

use super::*;

impl Archive {
    /// Dismisses or restores a generated cluster as a local maintainer decision.
    pub async fn set_cluster_dismissed_fenced(
        &self,
        token: &ArchiveLeaseToken,
        id: u64,
        dismissed: bool,
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
        let event = if dismissed { "dismissed" } else { "restored" };
        let result = if dismissed {
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

    /// Excludes or includes one current generated member as a local decision.
    pub async fn set_cluster_member_excluded_fenced(
        &self,
        token: &ArchiveLeaseToken,
        id: u64,
        thread: &ThreadId,
        excluded: bool,
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
        sqlx::query(
            "INSERT INTO cluster_member_decisions (cluster_id, thread_id, excluded, reason, updated_at_us) VALUES (?, ?, ?, ?, ?) ON CONFLICT (cluster_id, thread_id) DO UPDATE SET excluded = excluded.excluded, reason = excluded.reason, updated_at_us = excluded.updated_at_us",
        )
        .bind(cluster_id)
        .bind(member_id)
        .bind(if excluded { 1_i64 } else { 0_i64 })
        .bind(reason.trim())
        .bind(at.unix_microseconds())
        .execute(&mut *transaction)
        .await?;
        let state = if excluded { "excluded" } else { "active" };
        sqlx::query("UPDATE cluster_memberships SET state = ?, updated_at_us = ? WHERE cluster_id = ? AND thread_id = ?")
            .bind(state)
            .bind(at.unix_microseconds())
            .bind(cluster_id)
            .bind(member_id)
            .execute(&mut *transaction)
            .await?;
        if excluded {
            sqlx::query("UPDATE clusters SET canonical_thread_id = NULL, updated_at_us = ? WHERE id = ? AND canonical_thread_id = ?")
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
            reason.trim(),
            at,
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Sets the canonical member while preserving the generated representative for future runs.
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

pub(super) async fn insert_cluster_event(
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

pub(super) fn checked_cluster_id(id: u64) -> Result<i64, StoreError> {
    i64::try_from(id)
        .ok()
        .filter(|id| *id > 0)
        .ok_or(StoreError::ClusterMissing)
}
