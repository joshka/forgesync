//! Durable workflow ledger: runs, sync jobs, and isolated failures.
//!
//! The ledger is separate from source observations: a failed job can be retried without
//! pretending its missing evidence was acquired.

use forgesync_core::content::Repository;
use forgesync_core::coverage::{EvidenceFamily, Failure};
use forgesync_core::identity::{RepositoryId, RunId};
use forgesync_core::outcome::OperationOutcome;
use forgesync_core::timestamp::UtcTimestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::StoreError;
use crate::sql::to_sql_integer;

/// Durable terminal or active state of one sync run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum SyncJobStatus {
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

/// Selected work recorded when a repository-family job begins.
pub struct SyncJobStart<'a> {
    /// Parent run under which this attempt is recorded.
    pub run_id: RunId,
    /// Registered stable repository identity, distinct from a selector or display path.
    pub repository: &'a RepositoryId,
    /// Independently acquired resource family selected for this job.
    pub family: EvidenceFamily,
    /// Sub-scope such as open or closed threads, preserved exactly in the ledger.
    pub scope_key: &'a str,
    /// Local start time, used for both initial start and update timestamps.
    pub started_at: UtcTimestamp,
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
    pub thread: Option<&'a forgesync_core::identity::ThreadId>,
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
    pub thread: &'a forgesync_core::identity::ThreadId,
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

mod failures;
mod lifecycle;
mod query;

/// Derives durable run status from terminal job outcomes.
fn run_status(outcome: &OperationOutcome) -> RunStatus {
    match outcome {
        OperationOutcome::Complete => RunStatus::Complete,
        OperationOutcome::Partial { .. } => RunStatus::Partial,
        OperationOutcome::Deferred { .. } => RunStatus::Deferred,
        OperationOutcome::Failed { .. } => RunStatus::Failed,
        OperationOutcome::Interrupted { .. } => RunStatus::Interrupted,
    }
}

/// Rejects a nonpositive stored run identity.
fn checked_run_id(value: i64) -> Result<RunId, StoreError> {
    u64::try_from(value)
        .ok()
        .and_then(|value| RunId::new(value).ok())
        .ok_or(StoreError::Corrupt("sync_run_invalid"))
}

/// Checks a run identity before binding it to SQLite.
fn to_sql_id(value: RunId) -> Result<i64, StoreError> {
    to_sql_integer(value.get())
}
