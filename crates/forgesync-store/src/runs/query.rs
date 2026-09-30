//! # Inspect run history and retryable failures
//!
//! These `Archive` reads assemble recent runs, job detail, and failures for CLI reporting and
//! engine retry planning. They expose typed projections from the ledger instead of making callers
//! join workflow tables.
//!
//! A retry should be based on recorded failure scope and the current archive state. Querying is
//! side-effect-free; starting a new run belongs to `lifecycle` and the engine coordinator.
//!
//! [`Archive::list_runs`] returns a bounded newest-first history, breaking equal start times by
//! descending durable run identity. [`Archive::run_detail`] first finds the run, then loads jobs
//! and failures in insertion-identity order. Resolved failures remain visible in historical detail;
//! this is a record of execution, not an unresolved-only retry selection.
//!
//! Detail uses separate pool reads without one enclosing transaction. A writer can finish a job
//! or resolve a failure between sections, so the assembled result is diagnostic current state
//! rather than a frozen run snapshot. Repository payloads are joined from current registrations,
//! while run scope and outcome retain their recorded JSON.
//!
//! Row decoders check domain IDs, timestamps, nonnegative counts, known status/family values, and
//! JSON representations before returning typed records. Malformed persisted facts reject the read
//! rather than silently removing the affected job or failure. Optional outcome/failure fields stay
//! absent when SQL stores null; no workflow result is inferred from a missing payload.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::RunId;
use sqlx::Row;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::runs::{
    RunDetail, RunFailureRecord, RunRecord, RunStatus, SyncJobRecord, SyncJobStatus,
    checked_run_id, decode_count, decode_timestamp, to_sql_id,
};

impl Archive {
    /// Lists at most `limit` runs by descending start time, then descending run identity.
    ///
    /// Includes active and terminal runs without filtering by outcome. This is a bounded history
    /// read, not offset pagination or a frozen view across repeated calls.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::InvalidSyncCount`] for limits outside `1..=1000`. Database and checked
    /// persisted-record decoding errors reject the read rather than returning a partial list.
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

    /// Loads a run, jobs, and historical failures, or returns `None` for an unknown run.
    ///
    /// Jobs and failures are ordered by ascending durable row identity. Failures include resolved
    /// entries. Repository descriptions come from current registered payloads, not snapshots at
    /// run creation. The run and child sections are separate reads, so a concurrent writer can
    /// advance between them; use this for inspection rather than mutation authority.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::IntegerOutOfRange`] when the checked run identity exceeds SQLite's
    /// signed range. Database or persisted-record decoding failures reject the whole projection;
    /// no partial detail is returned.
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

/// Converts a durable run row and its original scope to a typed record.
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

/// Converts a persisted family job row to a typed job record.
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

/// Converts a persisted failure row without exposing raw provider payloads.
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

/// Rejects an unsupported stored evidence family label.
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

/// Rejects an unsupported durable run status label.
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

/// Rejects an unsupported durable job status label.
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
