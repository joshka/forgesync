//! Evidence acquisition and archive application.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
use std::time::Duration;

use forgesync_core::content::{Comment, PullRequestMetadata, Review, ReviewThread, ThreadKind};
use forgesync_core::coverage::{DeferredReason, EvidenceFamily, Failure, FailureKind};
use forgesync_core::identity::{GitHubHost, RunId, ThreadId};
use forgesync_core::observation::{CollectionCompleteness, IncompleteReason, SourceClock};
use forgesync_core::outcome::OperationOutcome;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_github::error::GitHubError;
use forgesync_github::resources::{
    ThreadListState, fetch_issue_comment_page, fetch_pull_request_metadata,
    fetch_pull_request_review_page, fetch_repository,
};
use forgesync_github::review_threads::{GraphqlCursor, fetch_review_thread_page};
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use forgesync_store::enumeration::RepositoryThreadScanStatus;
use forgesync_store::error::StoreError;
use forgesync_store::families::ChildFamilyObservation;
use forgesync_store::leases::ArchiveLeaseToken;
use forgesync_store::observations::{ObservationDisposition, StagedItem};
use forgesync_store::reads::{ThreadQuery, ThreadSort, ThreadStateFilter};
use forgesync_store::runs::{
    ChildFamilyFailureScope, RunFailureInput, RunFailureScope, RunRecord, SyncJobCompletion,
    SyncJobRecord, SyncJobStatus,
};
use serde::Serialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio::time::{Instant, interval_at};
use tokio_util::sync::CancellationToken;

use crate::enumeration::{
    ThreadEnumerationReport, ThreadScanContext, enumerate_repository_thread_pages, github_failure,
    now_utc,
};
use crate::error::EngineError;
use crate::reference::RepositorySelector;

const ARCHIVE_LEASE_DURATION: Duration = Duration::from_secs(60);
const CLOSED_SWEEP_OVERLAP_MICROSECONDS: i64 = 86_400_000_000;

mod comments;
mod jobs;
mod metadata;
mod pull_requests;
mod review_threads;
mod reviews;
mod support;

use comments::*;
use jobs::*;
use metadata::*;
use pull_requests::*;
use review_threads::*;
use reviews::*;
use support::*;

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
    /// Number of selected repository-family jobs in this run.
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
    /// Number of repository-family jobs selected by the run.
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

#[derive(Clone, Copy)]
struct ScopeUnit {
    key: &'static str,
    state: ThreadListState,
    update_closed_watermark: bool,
}

struct WorkSummary {
    total_jobs: u64,
    completed_jobs: u64,
    failed_jobs: u64,
    deferred_jobs: u64,
    pages_completed: u64,
    threads_seen: u64,
    comments_seen: u64,
    pull_request_metadata_seen: u64,
    reviews_seen: u64,
    review_threads_seen: u64,
    interrupted: bool,
    interrupted_jobs: u64,
    pending_jobs: u64,
    first_failure: Option<Failure>,
}

struct SyncRunContext<'a> {
    total_jobs: u64,
    include_comments: bool,
    include_reviews: bool,
    include_review_threads: bool,
    run_id: RunId,
    lease: &'a ArchiveLeaseToken,
    cancellation: &'a CancellationToken,
    progress: Option<mpsc::Sender<SyncProgress>>,
}

impl SyncThreadScope {
    fn units(self) -> Vec<ScopeUnit> {
        match self {
            Self::Default => vec![
                ScopeUnit {
                    key: "open",
                    state: ThreadListState::Open,
                    update_closed_watermark: false,
                },
                ScopeUnit {
                    key: "closed",
                    state: ThreadListState::Closed,
                    update_closed_watermark: true,
                },
            ],
            Self::Open => vec![ScopeUnit {
                key: "open",
                state: ThreadListState::Open,
                update_closed_watermark: false,
            }],
            Self::Closed => vec![ScopeUnit {
                key: "closed",
                state: ThreadListState::Closed,
                update_closed_watermark: true,
            }],
            Self::All => vec![ScopeUnit {
                key: "all",
                state: ThreadListState::All,
                update_closed_watermark: true,
            }],
        }
    }
}

/// Runs a fenced, resumable sync and returns a durable partial or complete report.
///
/// Progress updates are snapshots sent with `try_send`; a full or disconnected channel never
/// blocks archive writes or changes the final report.
pub async fn sync_repositories(
    archive: &Archive,
    clients: &HashMap<GitHubHost, GitHubClient>,
    request: &SyncRequest,
    cancellation: &CancellationToken,
    progress: Option<mpsc::Sender<SyncProgress>>,
) -> Result<SyncReport, EngineError> {
    if (request.all && !request.repositories.is_empty())
        || (!request.all && request.repositories.is_empty())
    {
        return Err(EngineError::InvalidSyncScope);
    }

    let selectors = resolve_selectors(archive, request).await?;
    let mut unique_selectors = Vec::with_capacity(selectors.len());
    let mut seen = HashSet::new();
    for selector in selectors {
        if seen.insert(selector.clone()) {
            if !clients.contains_key(selector.host()) {
                return Err(EngineError::GitHubClientMissing {
                    host: selector.host().as_str().to_owned(),
                });
            }
            unique_selectors.push(selector);
        }
    }

    let units = request.scope.units();
    let total_jobs = unique_selectors
        .len()
        .checked_mul(units.len())
        .and_then(|count| count.checked_mul(1 + usize::from(request.include_comments)))
        .and_then(|count| u64::try_from(count).ok())
        .ok_or(StoreError::IntegerOutOfRange)?;
    let started_at = now_utc()?;
    let lease = archive
        .acquire_archive_lease(started_at, ARCHIVE_LEASE_DURATION)
        .await?;
    let run_scope = json!({
        "repositories": unique_selectors.iter().map(RepositorySelector::as_url).collect::<Vec<_>>(),
        "all": request.all,
        "thread_scope": request.scope,
        "include_comments": request.include_comments,
        "include_reviews": request.include_reviews,
        "include_review_threads": request.include_review_threads,
    });
    let run_id = match archive
        .create_run(&lease, request.parent_run, started_at, &run_scope)
        .await
    {
        Ok(run_id) => run_id,
        Err(error) => {
            let _ = archive.release_archive_lease(&lease, started_at).await;
            return Err(error.into());
        }
    };

    let operation_cancellation = cancellation.child_token();
    let mut operation = Box::pin(execute_and_finalize(
        archive,
        clients,
        &unique_selectors,
        &units,
        SyncRunContext {
            total_jobs,
            include_comments: request.include_comments,
            include_reviews: request.include_reviews,
            include_review_threads: request.include_review_threads,
            run_id,
            lease: &lease,
            cancellation: &operation_cancellation,
            progress,
        },
    ));
    let heartbeat_interval = ARCHIVE_LEASE_DURATION / 3;
    let mut heartbeat = interval_at(Instant::now() + heartbeat_interval, heartbeat_interval);
    let result = loop {
        tokio::select! {
            result = &mut operation => break result,
            _ = heartbeat.tick() => {
                let now = match now_utc() {
                    Ok(now) => now,
                    Err(error) => {
                        operation_cancellation.cancel();
                        let _ = operation.await;
                        break Err(error);
                    }
                };
                if let Err(error) = archive
                    .heartbeat_archive_lease(&lease, now, ARCHIVE_LEASE_DURATION)
                    .await
                {
                    operation_cancellation.cancel();
                    let _ = operation.await;
                    break Err(error.into());
                }
            }
        }
    };
    let release_at = now_utc()?;
    let _ = archive.release_archive_lease(&lease, release_at).await;
    result
}

async fn execute_and_finalize(
    archive: &Archive,
    clients: &HashMap<GitHubHost, GitHubClient>,
    selectors: &[RepositorySelector],
    units: &[ScopeUnit],
    context: SyncRunContext<'_>,
) -> Result<SyncReport, EngineError> {
    let work = match run_jobs(archive, clients, selectors, units, &context).await {
        Ok(work) => work,
        Err(original_error) => {
            let failure = Failure {
                kind: FailureKind::Archive,
                message: "sync stopped before its work summary could be persisted".to_owned(),
            };
            let outcome = OperationOutcome::Failed { failure };
            if let Ok(finished_at) = now_utc() {
                let _ = archive
                    .finish_run(context.lease, context.run_id, finished_at, &outcome)
                    .await;
            }
            return Err(original_error);
        }
    };
    let outcome = operation_outcome(&work);
    archive
        .finish_run(context.lease, context.run_id, now_utc()?, &outcome)
        .await?;
    let detail = archive
        .run_detail(context.run_id)
        .await?
        .ok_or(StoreError::RunMissing)?;
    Ok(SyncReport {
        run: detail.run,
        jobs: detail.jobs,
        failures: detail.failures,
        repositories_selected: u64::try_from(selectors.len())
            .map_err(|_| StoreError::IntegerOutOfRange)?,
        completed_jobs: work.completed_jobs,
        total_jobs: work.total_jobs,
        failed_jobs: work.failed_jobs,
        deferred_jobs: work.deferred_jobs,
        pages_completed: work.pages_completed,
        threads_seen: work.threads_seen,
        comments_seen: work.comments_seen,
        pull_request_metadata_seen: work.pull_request_metadata_seen,
        reviews_seen: work.reviews_seen,
        review_threads_seen: work.review_threads_seen,
        outcome,
    })
}

#[derive(Default)]
struct CommentThreadResult {
    pages_completed: u64,
    comments_received: u64,
    comments_committed: u64,
    failure: Option<Failure>,
    interrupted: bool,
}

struct ThreadFamilyResult<T> {
    pages_completed: u64,
    items_received: u64,
    items_committed: u64,
    value: Option<T>,
    failure: Option<Failure>,
    interrupted: bool,
}

impl<T> Default for ThreadFamilyResult<T> {
    fn default() -> Self {
        Self {
            pages_completed: 0,
            items_received: 0,
            items_committed: 0,
            value: None,
            failure: None,
            interrupted: false,
        }
    }
}

#[derive(Default)]
struct FamilyJobAccumulator {
    pages_completed: u64,
    items_committed: u64,
    hard_failure: Option<Failure>,
    deferred_failure: Option<Failure>,
    interrupted: bool,
}

struct PullRequestTarget {
    thread: ThreadId,
    updated_at: UtcTimestamp,
}

#[derive(Clone, Copy)]
struct ThreadFamilyScope<'a> {
    repository: &'a forgesync_core::content::Repository,
    thread: &'a ThreadId,
    updated_at: UtcTimestamp,
    key: &'a str,
}
