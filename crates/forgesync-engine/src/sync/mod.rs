//! Acquire GitHub discussion evidence into an archive.
//!
//! One run visits each selected repository in order: a parent thread scan per thread-state unit,
//! then the selected child families (comments, pull-request metadata, reviews, review threads).
//! Each child family is reserved and staged independently per discussion; an incomplete
//! collection never replaces prior complete membership, and one discussion's failure does not
//! discard sibling work.

use forgesync_core::identity::RunId;
use forgesync_core::outcome::OperationOutcome;
use forgesync_store::runs::{RunRecord, SyncJobRecord};
use serde::{Deserialize, Serialize};

use crate::reference::RepositorySelector;

mod accounting;
mod coordinator;
mod families;
mod repository;

pub use coordinator::sync_repositories;

/// Thread scope requested for one sync run.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncThreadScope {
    /// Fetch open threads and run the durable closed-thread sweep.
    #[default]
    Default,
    /// Fetch only open threads.
    Open,
    /// Fetch closed threads changed since the last complete sweep, with overlap.
    Closed,
    /// Fetch all open and closed threads in one full enumeration.
    All,
}

/// Explicit repository selection and thread scope for one sync run.
///
/// Set `all` to select repositories already known to the archive, or provide explicit
/// [`RepositorySelector`] values. `parent_run` is reserved for a retry derived from durable run
/// history. Resource inclusion controls independent evidence families: omitting one leaves its
/// previous coverage intact rather than recording an empty collection.
#[derive(Clone, Debug)]
pub struct SyncRequest {
    /// Explicit repositories; required unless `all` is true.
    pub repositories: Vec<RepositorySelector>,
    /// Select all repositories already registered in the archive.
    pub all: bool,
    /// Thread state and closed-sweep policy.
    pub scope: SyncThreadScope,
    /// Acquire issue and pull-request discussion comments.
    pub include_comments: bool,
    /// Acquire pull-request reviews.
    pub include_reviews: bool,
    /// Acquire current pull-request review threads and nested comments.
    pub include_review_threads: bool,
    /// Parent run when this request explicitly retries durable work.
    pub parent_run: Option<RunId>,
}

/// Progress snapshot sent opportunistically through a bounded channel.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SyncProgress {
    /// Current run ID.
    pub run_id: RunId,
    /// Number of repository-family jobs that reached a terminal state.
    pub completed_jobs: u64,
    /// Current selected job count, growing as pull-request family work is discovered.
    pub total_jobs: u64,
    /// Number of discussion rows returned by committed provider pages.
    pub threads_seen: u64,
    /// Number of comments returned by committed provider pages.
    pub comments_seen: u64,
    /// Number of pull-request metadata records returned by successful requests.
    pub pull_request_metadata_seen: u64,
    /// Number of reviews returned by committed provider pages.
    pub reviews_seen: u64,
    /// Number of review threads returned by fully acquired GraphQL pages.
    pub review_threads_seen: u64,
    /// Current repository URL, when one is being processed.
    pub repository: Option<String>,
    /// State of the latest progress update.
    pub status: SyncProgressStatus,
}

/// Progress state for a repository-family job.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncProgressStatus {
    /// Provider metadata or thread pages are being acquired.
    InProgress,
    /// The selected scope completed.
    Complete,
    /// The selected scope failed.
    Failed,
    /// The selected scope was deferred by the retry budget.
    Deferred,
    /// Caller cancellation stopped the selected scope.
    Interrupted,
}

/// Final durable run report and aggregate acquisition counts.
///
/// Inspect [`Self::outcome`] and [`Self::failures`] together. Counts describe successfully
/// committed pages and records, so a partial run can contain useful new evidence while some jobs
/// remain failed or deferred. [`Self::run`] carries the persisted identity for later inspection
/// and retry planning.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SyncReport {
    /// Persisted run state and complete original scope.
    pub run: RunRecord,
    /// Persisted repository-family job records.
    pub jobs: Vec<SyncJobRecord>,
    /// Persisted failures, including failures before repository identity resolution.
    pub failures: Vec<forgesync_store::runs::RunFailureRecord>,
    /// Number of repositories selected by this request.
    pub repositories_selected: u64,
    /// Number of jobs that reached a terminal state.
    pub completed_jobs: u64,
    /// Total selected jobs, including pull-request family jobs added for nonempty scopes.
    pub total_jobs: u64,
    /// Number of failed jobs.
    pub failed_jobs: u64,
    /// Number of deferred jobs.
    pub deferred_jobs: u64,
    /// Number of fully committed provider pages across REST and GraphQL families.
    pub pages_completed: u64,
    /// Number of discussion rows returned by committed pages.
    pub threads_seen: u64,
    /// Number of comments returned by committed pages.
    pub comments_seen: u64,
    /// Number of pull-request metadata records returned by successful requests.
    pub pull_request_metadata_seen: u64,
    /// Number of reviews returned by committed pages.
    pub reviews_seen: u64,
    /// Number of review threads returned by fully acquired GraphQL pages.
    pub review_threads_seen: u64,
    /// Terminal outcome persisted on the run.
    pub outcome: OperationOutcome,
}

/// Original request scope persisted on a run and read back for retry planning.
#[derive(Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct RunScope {
    /// Normalized repository URLs selected by the run.
    pub repositories: Vec<String>,
    /// Whether the run selected all registered repositories.
    pub all: bool,
    pub thread_scope: SyncThreadScope,
    pub include_comments: bool,
    pub include_reviews: bool,
    pub include_review_threads: bool,
}

impl RunScope {
    /// Decodes a persisted scope; unreadable data enables no additional acquisition.
    pub fn decode(value: &serde_json::Value) -> Self {
        Self::deserialize(value).unwrap_or_default()
    }
}
