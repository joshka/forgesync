//! # Acquire GitHub discussion evidence into an archive
//!
//! `SyncRequest` selects repositories, thread scope, and evidence families; `SyncReport` and
//! progress types expose completed, partial, and failed work. The engine receives an already
//! opened archive and explicit cancellation. It uses the GitHub adapter for transport and
//! normalization, then the store for ordered observations.
//!
//! `coordinator` validates selection and owns run finalization; `scope` defines shared acquisition
//! identities and family results. `lease` maintains the writer fence through cooperative
//! cancellation and cleanup. `accounting` owns run-wide counters and outcome selection; `jobs`
//! coordinates thread work. `comments`, `reviews`, and `review_threads` own independently paginated
//! child families; `pull_requests` and `metadata` handle pull-request-specific evidence.
//! `review_collection` owns the reserved lifecycle shared by review families, while their provider
//! collectors own page traversal. `support` resolves selectors and records scoped failures. An
//! incomplete child collection must not replace prior complete membership. Per-job failure
//! isolation lets one discussion fail while other work still commits.

use forgesync_core::identity::RunId;
use forgesync_core::outcome::OperationOutcome;
use forgesync_store::runs::{RunRecord, SyncJobRecord};
use serde::Serialize;

use crate::reference::RepositorySelector;

mod accounting;
mod comment_job;
mod comments;
mod coordinator;
mod family_job;
mod jobs;
mod lease;
mod metadata;
mod pull_requests;
mod repository_work;
mod review_collection;
mod review_threads;
mod reviews;
mod scope;
mod support;
mod thread_job;

pub use coordinator::sync_repositories;

/// Thread scope requested for one sync run.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize)]
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
