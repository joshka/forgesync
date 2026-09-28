use forgesync_core::{
    EvidenceFamily, Failure, OperationOutcome, Repository, RepositoryId, RunId, UtcTimestamp,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;

use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::observations::{evidence_family_name, repository_row_id};
use crate::{Archive, StoreError};

/// Durable terminal or active state of one sync run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// The operation owns pending work or has recoverable work in progress.
    InProgress,
    /// Every selected work item completed.
    Complete,
    /// Some work completed while another selected item failed or was deferred.
    Partial,
    /// The operation failed before producing a reportable partial result.
    Failed,
    /// The caller stopped the operation while work remained.
    Interrupted,
    /// Policy deferred the operation without beginning selected work.
    Deferred,
}

/// Durable status for one repository and evidence-family job in a run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncJobStatus {
    /// Selected work is waiting to begin.
    Pending,
    /// The job is actively acquiring or applying source data.
    InProgress,
    /// The selected family completed and committed.
    Complete,
    /// The selected family could not be completed.
    Failed,
    /// The selected family remains for a later retry.
    Deferred,
    /// The run was interrupted while this job remained pending.
    Interrupted,
}

/// Durable summary of one operation and its original selected scope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    /// Stable archive-local run identity.
    pub id: RunId,
    /// Parent run for explicit retry or continuation, when present.
    pub parent_id: Option<RunId>,
    /// Current run state.
    pub status: RunStatus,
    /// Time when work began.
    pub started_at: UtcTimestamp,
    /// Time of the latest durable run update.
    pub updated_at: UtcTimestamp,
    /// Time when the run reached a terminal state.
    pub finished_at: Option<UtcTimestamp>,
    /// Explicit repository and family scope recorded before acquisition.
    pub scope: Value,
    /// Final reportable outcome when the run is terminal.
    pub outcome: Option<OperationOutcome>,
}

/// Durable result for one repository and family in a run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SyncJobRecord {
    /// Archive-local job row ID.
    pub id: i64,
    /// Parent run identity.
    pub run_id: RunId,
    /// Repository identity resolved by the provider or local archive.
    pub repository: Repository,
    /// Evidence family acquired by this job.
    pub family: EvidenceFamily,
    /// Sub-scope for family variants such as open and closed thread sweeps.
    pub scope_key: String,
    /// Current job state.
    pub status: SyncJobStatus,
    /// Time when this job began.
    pub started_at: UtcTimestamp,
    /// Time of the latest durable job update.
    pub updated_at: UtcTimestamp,
    /// Number of pages committed by this job.
    pub pages_completed: u64,
    /// Number of provider items committed by this job.
    pub items_committed: u64,
    /// Safe structured failure, when the job did not complete.
    pub failure: Option<Failure>,
}

/// A run and the repository-family jobs recorded under it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunDetail {
    /// Run metadata and final outcome.
    pub run: RunRecord,
    /// Selected work items and their durable state.
    pub jobs: Vec<SyncJobRecord>,
    /// Durable failures, including repository resolution failures without a repository row.
    pub failures: Vec<RunFailureRecord>,
}

/// Durable safe failure summary for one run scope or repository-family job.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunFailureRecord {
    /// Stable ledger row ID.
    pub id: i64,
    /// Repository or selector string that identifies the failed scope.
    pub target: String,
    /// Resolved repository identity when the provider repository was already known.
    pub repository: Option<Repository>,
    /// Evidence family when one was selected.
    pub family: Option<EvidenceFamily>,
    /// Stable provider ID for a selected thread, when the failure is thread-specific.
    pub thread_provider_id: Option<String>,
    /// Issue or pull-request number for a selected thread, when known.
    pub thread_number: Option<u64>,
    /// Sub-scope for variants such as open and closed thread sweeps.
    pub scope_key: String,
    /// Safe failure class and message.
    pub failure: Failure,
    /// Time when the ledger entry was stored.
    pub created_at: UtcTimestamp,
    /// Number of later runs that retried this unresolved failure.
    pub retry_count: u64,
    /// Time when a later run successfully resolved this failure.
    pub resolved_at: Option<UtcTimestamp>,
    /// Most recent run that retried this failure.
    pub retry_run_id: Option<RunId>,
}

/// Values needed to finish one sync job without spreading status fields across arguments.
pub struct SyncJobCompletion<'a> {
    /// Terminal state for the selected job.
    pub status: SyncJobStatus,
    /// Time when the final job state was written.
    pub updated_at: UtcTimestamp,
    /// Number of pages committed by this job.
    pub pages_completed: u64,
    /// Number of provider items committed by this job.
    pub items_committed: u64,
    /// Safe failure summary for a failed, deferred, or interrupted job.
    pub failure: Option<&'a Failure>,
}

/// Values needed to record a scope-level or thread-specific acquisition failure.
pub struct RunFailureInput<'a> {
    /// Parent run identity.
    pub run_id: RunId,
    /// Repository URL or other safe scope identifier.
    pub target: &'a str,
    /// Stable repository identity, when the source repository is known.
    pub repository: Option<&'a RepositoryId>,
    /// Stable thread identity for an independently retried child family.
    pub thread: Option<&'a forgesync_core::ThreadId>,
    /// Evidence family when the failed work selected one.
    pub family: Option<EvidenceFamily>,
    /// Sub-scope such as open or closed threads.
    pub scope_key: &'a str,
    /// Safe structured failure summary.
    pub failure: &'a Failure,
    /// Time when the failure was recorded.
    pub created_at: UtcTimestamp,
}

/// Identity and scope for retry or resolution of one child-family failure.
pub struct ChildFamilyFailureScope<'a> {
    /// Run that is attempting the selected family.
    pub run_id: RunId,
    /// Stable repository identity.
    pub repository: &'a RepositoryId,
    /// Stable discussion identity.
    pub thread: &'a forgesync_core::ThreadId,
    /// Evidence family being retried.
    pub family: EvidenceFamily,
    /// Scope key used when the failure was recorded.
    pub scope_key: &'a str,
}

/// Identity for a repository-selector failure that predates repository resolution.
pub struct RunFailureScope<'a> {
    /// Run attempting the selected scope.
    pub run_id: RunId,
    /// Exact repository selector URL recorded in the failure ledger.
    pub target: &'a str,
    /// Evidence family selected by the failed work.
    pub family: EvidenceFamily,
    /// Thread sub-scope recorded on the failure.
    pub scope_key: &'a str,
}

impl Archive {
    /// Marks matching unresolved selector failures as retried by this run.
    pub async fn mark_scope_failures_retried(
        &self,
        token: &ArchiveLeaseToken,
        scope: &RunFailureScope<'_>,
    ) -> Result<u64, StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let result = sqlx::query(
            "UPDATE failures SET retry_count = retry_count + 1, retry_run_id = ? WHERE repository_id IS NULL AND target_key = ? AND family = ? AND scope_key = ? AND resolved_at_us IS NULL AND run_id <> ?",
        )
        .bind(to_sql_id(scope.run_id)?)
        .bind(scope.target)
        .bind(evidence_family_name(scope.family))
        .bind(scope.scope_key)
        .bind(to_sql_id(scope.run_id)?)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(result.rows_affected())
    }

    /// Resolves selector failures only after the matching scope has completed.
    pub async fn resolve_scope_failures(
        &self,
        token: &ArchiveLeaseToken,
        scope: &RunFailureScope<'_>,
        resolved_at: UtcTimestamp,
    ) -> Result<u64, StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let result = sqlx::query(
            "UPDATE failures SET resolved_at_us = ?, retry_run_id = COALESCE(retry_run_id, ?) WHERE repository_id IS NULL AND target_key = ? AND family = ? AND scope_key = ? AND resolved_at_us IS NULL AND run_id <> ?",
        )
        .bind(resolved_at.unix_microseconds())
        .bind(to_sql_id(scope.run_id)?)
        .bind(scope.target)
        .bind(evidence_family_name(scope.family))
        .bind(scope.scope_key)
        .bind(to_sql_id(scope.run_id)?)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(result.rows_affected())
    }

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

    /// Records a failure that occurred before a repository identity could be resolved.
    pub async fn record_run_failure(
        &self,
        token: &ArchiveLeaseToken,
        failure: RunFailureInput<'_>,
    ) -> Result<(), StoreError> {
        if let Some(thread) = failure.thread {
            let Some(repository) = failure.repository else {
                return Err(StoreError::InvalidRunData);
            };
            if repository != thread.repository() {
                return Err(StoreError::InvalidRunData);
            }
        }
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let failure_json = serde_json::to_string(failure.failure)?;
        let repository_id = match failure.repository {
            Some(repository) => Some(
                repository_row_id(
                    &mut transaction,
                    repository.host().as_str(),
                    repository.provider_id().as_str(),
                )
                .await?,
            ),
            None => None,
        };
        let thread_number = failure
            .thread
            .map(|thread| to_sql_id_u64(thread.number().get()))
            .transpose()?;
        let thread_provider_id = failure.thread.map(|thread| thread.provider_id().as_str());
        sqlx::query(
            "INSERT INTO failures (run_id, repository_id, family, target_key, scope_key, failure_json, created_at_us, thread_provider_id, thread_number) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(to_sql_id(failure.run_id)?)
        .bind(repository_id)
        .bind(failure.family.map(evidence_family_name))
        .bind(failure.target)
        .bind(failure.scope_key)
        .bind(failure_json)
        .bind(failure.created_at.unix_microseconds())
        .bind(thread_provider_id)
        .bind(thread_number)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Marks unresolved thread-family failures as retried by this run.
    pub async fn mark_child_family_failures_retried(
        &self,
        token: &ArchiveLeaseToken,
        scope: &ChildFamilyFailureScope<'_>,
    ) -> Result<u64, StoreError> {
        if scope.repository != scope.thread.repository() {
            return Err(StoreError::InvalidRunData);
        }
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let repository_id = repository_row_id(
            &mut transaction,
            scope.repository.host().as_str(),
            scope.repository.provider_id().as_str(),
        )
        .await?;
        let result = sqlx::query(
            "UPDATE failures SET retry_count = retry_count + 1, retry_run_id = ? WHERE repository_id = ? AND thread_provider_id = ? AND thread_number = ? AND family = ? AND scope_key = ? AND resolved_at_us IS NULL AND run_id <> ?",
        )
        .bind(to_sql_id(scope.run_id)?)
        .bind(repository_id)
        .bind(scope.thread.provider_id().as_str())
        .bind(to_sql_id_u64(scope.thread.number().get())?)
        .bind(evidence_family_name(scope.family))
        .bind(scope.scope_key)
        .bind(to_sql_id(scope.run_id)?)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(result.rows_affected())
    }

    /// Resolves prior failures only after the matching child family is complete.
    pub async fn resolve_child_family_failures(
        &self,
        token: &ArchiveLeaseToken,
        scope: &ChildFamilyFailureScope<'_>,
        resolved_at: UtcTimestamp,
    ) -> Result<u64, StoreError> {
        if scope.repository != scope.thread.repository() {
            return Err(StoreError::InvalidRunData);
        }
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let repository_id = repository_row_id(
            &mut transaction,
            scope.repository.host().as_str(),
            scope.repository.provider_id().as_str(),
        )
        .await?;
        let result = sqlx::query(
            "UPDATE failures SET resolved_at_us = ?, retry_run_id = COALESCE(retry_run_id, ?) WHERE repository_id = ? AND thread_provider_id = ? AND thread_number = ? AND family = ? AND scope_key = ? AND resolved_at_us IS NULL AND run_id <> ?",
        )
        .bind(resolved_at.unix_microseconds())
        .bind(to_sql_id(scope.run_id)?)
        .bind(repository_id)
        .bind(scope.thread.provider_id().as_str())
        .bind(to_sql_id_u64(scope.thread.number().get())?)
        .bind(evidence_family_name(scope.family))
        .bind(scope.scope_key)
        .bind(to_sql_id(scope.run_id)?)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(result.rows_affected())
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

    /// Lists recent runs in stable newest-first order.
    pub async fn list_runs(&self, limit: u32) -> Result<Vec<RunRecord>, StoreError> {
        if limit == 0 || limit > 1000 {
            return Err(StoreError::InvalidSyncCount);
        }
        let rows = sqlx::query(
            "SELECT id, parent_run_id, status, started_at_us, updated_at_us, finished_at_us, scope_json, outcome_json FROM runs ORDER BY started_at_us DESC, id DESC LIMIT ?",
        )
        .bind(i64::from(limit))
        .fetch_all(&self.reader)
        .await?;
        rows.into_iter().map(decode_run).collect()
    }

    /// Loads a run and its jobs, or returns `None` when the ID is unknown.
    pub async fn run_detail(&self, run_id: RunId) -> Result<Option<RunDetail>, StoreError> {
        let row = sqlx::query(
            "SELECT id, parent_run_id, status, started_at_us, updated_at_us, finished_at_us, scope_json, outcome_json FROM runs WHERE id = ?",
        )
        .bind(to_sql_id(run_id)?)
        .fetch_optional(&self.reader)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let run = decode_run(row)?;
        let rows = sqlx::query(
            "SELECT j.id, j.run_id, r.payload_json AS repository_json, j.family, j.scope_key, j.status, j.started_at_us, j.updated_at_us, j.pages_completed, j.items_committed, j.failure_json FROM jobs j JOIN repositories r ON r.id = j.repository_id WHERE j.run_id = ? ORDER BY j.id",
        )
        .bind(to_sql_id(run_id)?)
        .fetch_all(&self.reader)
        .await?;
        let jobs = rows.into_iter().map(decode_job).collect::<Result<_, _>>()?;
        let rows = sqlx::query(
            "SELECT f.id, f.target_key, f.family, f.scope_key, f.failure_json, f.created_at_us, f.thread_provider_id, f.thread_number, f.retry_count, f.resolved_at_us, f.retry_run_id, r.payload_json AS repository_json FROM failures f LEFT JOIN repositories r ON r.id = f.repository_id WHERE f.run_id = ? ORDER BY f.id",
        )
        .bind(to_sql_id(run_id)?)
        .fetch_all(&self.reader)
        .await?;
        let failures = rows
            .into_iter()
            .map(decode_failure)
            .collect::<Result<_, _>>()?;
        Ok(Some(RunDetail {
            run,
            jobs,
            failures,
        }))
    }
}

fn decode_run(row: sqlx::sqlite::SqliteRow) -> Result<RunRecord, StoreError> {
    let raw_id: i64 = row.try_get("id")?;
    let parent_id: Option<i64> = row.try_get("parent_run_id")?;
    let status: String = row.try_get("status")?;
    let started_at = decode_timestamp(row.try_get("started_at_us")?)?;
    let updated_at = decode_timestamp(row.try_get("updated_at_us")?)?;
    let finished_at = row
        .try_get::<Option<i64>, _>("finished_at_us")?
        .map(decode_timestamp)
        .transpose()?;
    let scope_json: String = row.try_get("scope_json")?;
    let outcome_json: Option<String> = row.try_get("outcome_json")?;
    Ok(RunRecord {
        id: checked_run_id(raw_id)?,
        parent_id: parent_id.map(checked_run_id).transpose()?,
        status: parse_run_status(&status)?,
        started_at,
        updated_at,
        finished_at,
        scope: serde_json::from_str(&scope_json)?,
        outcome: outcome_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?,
    })
}

fn decode_job(row: sqlx::sqlite::SqliteRow) -> Result<SyncJobRecord, StoreError> {
    let run_id: i64 = row.try_get("run_id")?;
    let repository_json: String = row.try_get("repository_json")?;
    let family: String = row.try_get("family")?;
    let status: String = row.try_get("status")?;
    let failure_json: Option<String> = row.try_get("failure_json")?;
    let pages_completed = decode_count(row.try_get("pages_completed")?)?;
    let items_committed = decode_count(row.try_get("items_committed")?)?;
    Ok(SyncJobRecord {
        id: row.try_get("id")?,
        run_id: checked_run_id(run_id)?,
        repository: serde_json::from_str(&repository_json)?,
        family: parse_family(&family)?,
        scope_key: row.try_get("scope_key")?,
        status: parse_job_status(&status)?,
        started_at: decode_timestamp(row.try_get("started_at_us")?)?,
        updated_at: decode_timestamp(row.try_get("updated_at_us")?)?,
        pages_completed,
        items_committed,
        failure: failure_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?,
    })
}

fn decode_failure(row: sqlx::sqlite::SqliteRow) -> Result<RunFailureRecord, StoreError> {
    let family: Option<String> = row.try_get("family")?;
    let failure_json: String = row.try_get("failure_json")?;
    let thread_number: Option<i64> = row.try_get("thread_number")?;
    let retry_run_id: Option<i64> = row.try_get("retry_run_id")?;
    let repository_json: Option<String> = row.try_get("repository_json")?;
    Ok(RunFailureRecord {
        id: row.try_get("id")?,
        target: row.try_get("target_key")?,
        repository: repository_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?,
        family: family.map(|family| parse_family(&family)).transpose()?,
        thread_provider_id: row.try_get("thread_provider_id")?,
        thread_number: thread_number
            .map(|number| u64::try_from(number).map_err(|_| StoreError::InvalidRunData))
            .transpose()?,
        scope_key: row.try_get("scope_key")?,
        failure: serde_json::from_str(&failure_json)?,
        created_at: decode_timestamp(row.try_get("created_at_us")?)?,
        retry_count: decode_count(row.try_get("retry_count")?)?,
        resolved_at: row
            .try_get::<Option<i64>, _>("resolved_at_us")?
            .map(decode_timestamp)
            .transpose()?,
        retry_run_id: retry_run_id.map(checked_run_id).transpose()?,
    })
}

fn parse_family(value: &str) -> Result<EvidenceFamily, StoreError> {
    match value {
        "threads" => Ok(EvidenceFamily::Threads),
        "comments" => Ok(EvidenceFamily::Comments),
        "pull_request_metadata" => Ok(EvidenceFamily::PullRequestMetadata),
        "reviews" => Ok(EvidenceFamily::Reviews),
        "review_threads" => Ok(EvidenceFamily::ReviewThreads),
        _ => Err(StoreError::InvalidRunData),
    }
}

fn parse_run_status(value: &str) -> Result<RunStatus, StoreError> {
    match value {
        "in_progress" => Ok(RunStatus::InProgress),
        "complete" => Ok(RunStatus::Complete),
        "partial" => Ok(RunStatus::Partial),
        "failed" => Ok(RunStatus::Failed),
        "interrupted" => Ok(RunStatus::Interrupted),
        "deferred" => Ok(RunStatus::Deferred),
        _ => Err(StoreError::InvalidRunData),
    }
}

fn parse_job_status(value: &str) -> Result<SyncJobStatus, StoreError> {
    match value {
        "pending" => Ok(SyncJobStatus::Pending),
        "in_progress" => Ok(SyncJobStatus::InProgress),
        "complete" => Ok(SyncJobStatus::Complete),
        "failed" => Ok(SyncJobStatus::Failed),
        "deferred" => Ok(SyncJobStatus::Deferred),
        "interrupted" => Ok(SyncJobStatus::Interrupted),
        _ => Err(StoreError::InvalidRunData),
    }
}

fn run_status(outcome: &OperationOutcome) -> RunStatus {
    match outcome {
        OperationOutcome::Complete => RunStatus::Complete,
        OperationOutcome::Partial { .. } => RunStatus::Partial,
        OperationOutcome::Deferred { .. } => RunStatus::Deferred,
        OperationOutcome::Failed { .. } => RunStatus::Failed,
        OperationOutcome::Interrupted { .. } => RunStatus::Interrupted,
    }
}

fn run_status_name(status: RunStatus) -> &'static str {
    match status {
        RunStatus::InProgress => "in_progress",
        RunStatus::Complete => "complete",
        RunStatus::Partial => "partial",
        RunStatus::Failed => "failed",
        RunStatus::Interrupted => "interrupted",
        RunStatus::Deferred => "deferred",
    }
}

fn job_status_name(status: SyncJobStatus) -> &'static str {
    match status {
        SyncJobStatus::Pending => "pending",
        SyncJobStatus::InProgress => "in_progress",
        SyncJobStatus::Complete => "complete",
        SyncJobStatus::Failed => "failed",
        SyncJobStatus::Deferred => "deferred",
        SyncJobStatus::Interrupted => "interrupted",
    }
}

fn checked_run_id(value: i64) -> Result<RunId, StoreError> {
    let value = u64::try_from(value).map_err(|_| StoreError::InvalidRunData)?;
    RunId::new(value).map_err(|_| StoreError::InvalidRunData)
}

fn to_sql_id(value: RunId) -> Result<i64, StoreError> {
    i64::try_from(value.get()).map_err(|_| StoreError::IntegerOutOfRange)
}

fn to_sql_id_u64(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::IntegerOutOfRange)
}

fn decode_count(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::InvalidSyncCount)
}

fn decode_timestamp(value: i64) -> Result<UtcTimestamp, StoreError> {
    UtcTimestamp::from_unix_microseconds(value).map_err(StoreError::InvalidCreatedAt)
}
