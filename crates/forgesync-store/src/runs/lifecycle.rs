//! Run lifecycle operations.

use super::{
    Archive, ArchiveLeaseToken, EvidenceFamily, OperationOutcome, RepositoryId, RunId, StoreError,
    SyncJobCompletion, SyncJobStatus, UtcTimestamp, Value, checked_run_id, evidence_family_name,
    job_status_name, repository_row_id, require_active_archive_lease, run_status, run_status_name,
    to_sql_id, to_sql_id_u64,
};

impl Archive {
    /// Inserts a run before acquisition and records its complete requested scope.
    pub async fn create_run(
        &self,
        token: &ArchiveLeaseToken,
        parent_id: Option<RunId>,
        started_at: UtcTimestamp,
        scope: &Value,
    ) -> Result<RunId, StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let scope_json = serde_json::to_string(scope)?;
        let parent_id = parent_id.map(to_sql_id).transpose()?;
        let raw_id: i64 = sqlx::query_scalar(
            "INSERT INTO runs (parent_run_id, status, started_at_us, updated_at_us, finished_at_us, scope_json, outcome_json) VALUES (?, 'in_progress', ?, ?, NULL, ?, NULL) RETURNING id",
        )
        .bind(parent_id)
        .bind(started_at.unix_microseconds())
        .bind(started_at.unix_microseconds())
        .bind(scope_json)
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        checked_run_id(raw_id)
    }

    /// Starts a fenced job for one repository, evidence family, and sub-scope.
    pub async fn start_sync_job(
        &self,
        token: &ArchiveLeaseToken,
        run_id: RunId,
        repository: &RepositoryId,
        family: EvidenceFamily,
        scope_key: &str,
        started_at: UtcTimestamp,
    ) -> Result<i64, StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let repository_id = repository_row_id(
            &mut transaction,
            repository.host().as_str(),
            repository.provider_id().as_str(),
        )
        .await?;
        let job_id: i64 = sqlx::query_scalar(
            "INSERT INTO jobs (run_id, repository_id, family, scope_key, status, started_at_us, updated_at_us, pages_completed, items_committed, failure_json) VALUES (?, ?, ?, ?, 'in_progress', ?, ?, 0, 0, NULL) RETURNING id",
        )
        .bind(to_sql_id(run_id)?)
        .bind(repository_id)
        .bind(evidence_family_name(family))
        .bind(scope_key)
        .bind(started_at.unix_microseconds())
        .bind(started_at.unix_microseconds())
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(job_id)
    }

    /// Writes a job's terminal or interrupted state and adds its failure to the ledger.
    pub async fn finish_sync_job(
        &self,
        token: &ArchiveLeaseToken,
        job_id: i64,
        completion: SyncJobCompletion<'_>,
    ) -> Result<(), StoreError> {
        if matches!(
            completion.status,
            SyncJobStatus::Pending | SyncJobStatus::InProgress
        ) {
            return Err(StoreError::InvalidRunData);
        }
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let failure_json = completion.failure.map(serde_json::to_string).transpose()?;
        let result = sqlx::query(
            "UPDATE jobs SET status = ?, updated_at_us = ?, pages_completed = ?, items_committed = ?, failure_json = ? WHERE id = ? AND status = 'in_progress'",
        )
        .bind(job_status_name(completion.status))
        .bind(completion.updated_at.unix_microseconds())
        .bind(to_sql_id_u64(completion.pages_completed)?)
        .bind(to_sql_id_u64(completion.items_committed)?)
        .bind(failure_json.as_deref())
        .bind(job_id)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(StoreError::RunMissing);
        }
        if let Some(failure_json) = failure_json {
            sqlx::query(
                "INSERT INTO failures (run_id, job_id, repository_id, family, target_key, scope_key, failure_json, created_at_us) SELECT j.run_id, j.id, j.repository_id, j.family, r.full_name, j.scope_key, ?, ? FROM jobs j JOIN repositories r ON r.id = j.repository_id WHERE j.id = ?",
            )
            .bind(failure_json)
            .bind(completion.updated_at.unix_microseconds())
            .bind(job_id)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    /// Persists the final operation outcome under the current lease fence.
    pub async fn finish_run(
        &self,
        token: &ArchiveLeaseToken,
        run_id: RunId,
        updated_at: UtcTimestamp,
        outcome: &OperationOutcome,
    ) -> Result<(), StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let outcome_json = serde_json::to_string(outcome)?;
        let status = run_status(outcome);
        let result = sqlx::query(
            "UPDATE runs SET status = ?, updated_at_us = ?, finished_at_us = ?, outcome_json = ? WHERE id = ? AND status = 'in_progress'",
        )
        .bind(run_status_name(status))
        .bind(updated_at.unix_microseconds())
        .bind(updated_at.unix_microseconds())
        .bind(outcome_json)
        .bind(to_sql_id(run_id)?)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(StoreError::RunMissing);
        }
        transaction.commit().await?;
        Ok(())
    }
}
