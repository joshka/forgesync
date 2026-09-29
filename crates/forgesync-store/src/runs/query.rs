//! Run query operations.

use sqlx::Row;

use super::{
    Archive, EvidenceFamily, RunDetail, RunFailureRecord, RunId, RunRecord, RunStatus, StoreError,
    SyncJobRecord, SyncJobStatus, checked_run_id, decode_count, decode_timestamp, to_sql_id,
};

impl Archive {
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
