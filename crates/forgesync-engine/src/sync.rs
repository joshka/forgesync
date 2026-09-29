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
use forgesync_store::{
    Archive, ArchiveLeaseToken, ChildFamilyFailureScope, ChildFamilyObservation,
    ObservationDisposition, RepositoryThreadScanStatus, RunFailureInput, RunFailureScope,
    RunRecord, StagedItem, StoreError, SyncJobCompletion, SyncJobRecord, SyncJobStatus,
    ThreadQuery, ThreadSort, ThreadStateFilter,
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
use crate::{EngineError, RepositorySelector};

const ARCHIVE_LEASE_DURATION: Duration = Duration::from_secs(60);
const CLOSED_SWEEP_OVERLAP_MICROSECONDS: i64 = 86_400_000_000;

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
    pub failures: Vec<forgesync_store::RunFailureRecord>,
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

async fn run_jobs(
    archive: &Archive,
    clients: &HashMap<GitHubHost, GitHubClient>,
    selectors: &[RepositorySelector],
    units: &[ScopeUnit],
    context: &SyncRunContext<'_>,
) -> Result<WorkSummary, EngineError> {
    let mut summary = WorkSummary {
        total_jobs: context.total_jobs,
        completed_jobs: 0,
        failed_jobs: 0,
        deferred_jobs: 0,
        pages_completed: 0,
        threads_seen: 0,
        comments_seen: 0,
        pull_request_metadata_seen: 0,
        reviews_seen: 0,
        review_threads_seen: 0,
        interrupted: false,
        interrupted_jobs: 0,
        pending_jobs: 0,
        first_failure: None,
    };

    for selector in selectors {
        if context.cancellation.is_cancelled() {
            summary.interrupted = true;
            break;
        }
        let client =
            clients
                .get(selector.host())
                .ok_or_else(|| EngineError::GitHubClientMissing {
                    host: selector.host().as_str().to_owned(),
                })?;
        let repository_started_at = now_utc()?;
        let repository_sequence = archive
            .reserve_observation_sequence_fenced(repository_started_at, context.lease)
            .await?;
        let repository = match fetch_repository(
            client,
            selector.host(),
            selector.owner(),
            selector.name(),
            context.cancellation,
        )
        .await
        {
            Ok(repository) => repository,
            Err(GitHubError::Cancelled) => {
                summary.interrupted = true;
                break;
            }
            Err(error) => {
                let failure = github_failure(&error);
                for unit in units {
                    let selected_families = [
                        Some(EvidenceFamily::Threads),
                        context.include_comments.then_some(EvidenceFamily::Comments),
                    ]
                    .into_iter()
                    .flatten();
                    for family in selected_families {
                        archive
                            .record_run_failure(
                                context.lease,
                                RunFailureInput {
                                    run_id: context.run_id,
                                    target: &selector.as_url(),
                                    repository: None,
                                    thread: None,
                                    family: Some(family),
                                    scope_key: unit.key,
                                    failure: &failure,
                                    created_at: now_utc()?,
                                },
                            )
                            .await
                            .map_err(|source| EngineError::FailureLedger {
                                original: failure.clone(),
                                source,
                            })?;
                        count_failure(&mut summary, &failure);
                        summary.completed_jobs += 1;
                        send_progress(
                            &context.progress,
                            context.run_id,
                            &summary,
                            summary.total_jobs,
                            Some(selector.as_url()),
                            progress_status(&failure),
                        );
                    }
                }
                continue;
            }
        };
        archive
            .upsert_repository_fenced(&repository, context.lease)
            .await?;
        let selector_target = selector.as_url();

        for (unit_index, unit) in units.iter().enumerate() {
            if context.cancellation.is_cancelled() {
                summary.interrupted = true;
                break;
            }
            let (started_at, sequence) = if unit_index == 0 {
                (repository_started_at, repository_sequence)
            } else {
                let started_at = now_utc()?;
                let sequence = archive
                    .reserve_observation_sequence_fenced(started_at, context.lease)
                    .await?;
                (started_at, sequence)
            };
            let since = if unit.state == ThreadListState::Closed {
                archive
                    .closed_sweep_watermark(&repository.id)
                    .await?
                    .map(overlap_start)
            } else {
                None
            };
            let job_id = archive
                .start_sync_job(
                    context.lease,
                    context.run_id,
                    &repository.id,
                    EvidenceFamily::Threads,
                    unit.key,
                    started_at,
                )
                .await?;
            archive
                .mark_scope_failures_retried(
                    context.lease,
                    &RunFailureScope {
                        run_id: context.run_id,
                        target: &selector_target,
                        family: EvidenceFamily::Threads,
                        scope_key: unit.key,
                    },
                )
                .await?;
            send_progress(
                &context.progress,
                context.run_id,
                &summary,
                summary.total_jobs,
                Some(selector.as_url()),
                SyncProgressStatus::InProgress,
            );

            let report = enumerate_repository_thread_pages(
                archive,
                client,
                ThreadScanContext {
                    repository: repository.clone(),
                    sequence,
                    started_at,
                    state: unit.state,
                    since,
                },
                Some(context.lease),
                context.cancellation,
            )
            .await?;
            summary.pages_completed = summary
                .pages_completed
                .checked_add(report.scan.pages_completed)
                .ok_or(StoreError::IntegerOutOfRange)?;
            summary.threads_seen = summary
                .threads_seen
                .checked_add(report.scan.threads_seen)
                .ok_or(StoreError::IntegerOutOfRange)?;

            let (status, failure, progress_status) = job_result(&report);
            archive
                .finish_sync_job(
                    context.lease,
                    job_id,
                    SyncJobCompletion {
                        status,
                        updated_at: now_utc()?,
                        pages_completed: report.scan.pages_completed,
                        items_committed: report.scan.threads_seen,
                        failure: failure.as_ref(),
                    },
                )
                .await?;
            if report.scan.status == RepositoryThreadScanStatus::Complete
                && unit.update_closed_watermark
            {
                archive
                    .commit_closed_sweep_watermark(
                        context.lease,
                        &repository.id,
                        sequence,
                        started_at,
                        now_utc()?,
                    )
                    .await?;
            }
            if report.scan.status == RepositoryThreadScanStatus::Complete {
                archive
                    .resolve_scope_failures(
                        context.lease,
                        &RunFailureScope {
                            run_id: context.run_id,
                            target: &selector_target,
                            family: EvidenceFamily::Threads,
                            scope_key: unit.key,
                        },
                        now_utc()?,
                    )
                    .await?;
            }
            summary.completed_jobs += 1;
            if let Some(failure) = failure.as_ref() {
                count_failure(&mut summary, failure);
            }
            if report.interrupted {
                summary.interrupted = true;
                summary.interrupted_jobs += 1;
            }
            send_progress(
                &context.progress,
                context.run_id,
                &summary,
                summary.total_jobs,
                Some(selector.as_url()),
                progress_status,
            );
            if summary.interrupted {
                break;
            }
            if context.include_comments {
                run_comment_job(archive, client, &repository, *unit, context, &mut summary).await?;
                if summary.interrupted {
                    break;
                }
            }
            run_pull_request_jobs(archive, client, &repository, *unit, context, &mut summary)
                .await?;
            if summary.interrupted {
                break;
            }
        }
        if summary.interrupted {
            break;
        }
    }
    if summary.interrupted {
        summary.pending_jobs = summary
            .total_jobs
            .saturating_sub(summary.completed_jobs)
            .saturating_add(summary.interrupted_jobs);
    }
    Ok(summary)
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

async fn run_pull_request_jobs(
    archive: &Archive,
    client: &GitHubClient,
    repository: &forgesync_core::content::Repository,
    unit: ScopeUnit,
    context: &SyncRunContext<'_>,
    summary: &mut WorkSummary,
) -> Result<(), EngineError> {
    let targets = pull_request_targets(archive, repository, unit).await?;
    if targets.is_empty() {
        return Ok(());
    }
    let added_jobs =
        1 + u64::from(context.include_reviews) + u64::from(context.include_review_threads);
    summary.total_jobs = summary
        .total_jobs
        .checked_add(added_jobs)
        .ok_or(StoreError::IntegerOutOfRange)?;

    let started_at = now_utc()?;
    let metadata_job_id = archive
        .start_sync_job(
            context.lease,
            context.run_id,
            &repository.id,
            EvidenceFamily::PullRequestMetadata,
            unit.key,
            started_at,
        )
        .await?;
    let reviews_job_id = if context.include_reviews {
        Some(
            archive
                .start_sync_job(
                    context.lease,
                    context.run_id,
                    &repository.id,
                    EvidenceFamily::Reviews,
                    unit.key,
                    started_at,
                )
                .await?,
        )
    } else {
        None
    };
    let review_threads_job_id = if context.include_review_threads {
        Some(
            archive
                .start_sync_job(
                    context.lease,
                    context.run_id,
                    &repository.id,
                    EvidenceFamily::ReviewThreads,
                    unit.key,
                    started_at,
                )
                .await?,
        )
    } else {
        None
    };
    let progress_repository = RepositorySelector::from_repository(repository).as_url();
    send_progress(
        &context.progress,
        context.run_id,
        summary,
        summary.total_jobs,
        Some(progress_repository.clone()),
        SyncProgressStatus::InProgress,
    );

    let mut metadata_job = FamilyJobAccumulator::default();
    let mut reviews_job = FamilyJobAccumulator::default();
    let mut review_threads_job = FamilyJobAccumulator::default();
    for target in targets {
        if context.cancellation.is_cancelled() {
            metadata_job.interrupted = true;
            reviews_job.interrupted = context.include_reviews;
            review_threads_job.interrupted = context.include_review_threads;
            break;
        }
        let family_scope = ThreadFamilyScope {
            repository,
            thread: &target.thread,
            updated_at: target.updated_at,
            key: unit.key,
        };
        let metadata_result =
            sync_thread_pull_request_metadata(archive, client, &family_scope, context).await?;
        accumulate_thread_result(&mut metadata_job, &metadata_result)?;
        summary.pull_request_metadata_seen = summary
            .pull_request_metadata_seen
            .checked_add(metadata_result.items_received)
            .ok_or(StoreError::IntegerOutOfRange)?;

        if metadata_result.interrupted {
            metadata_job.interrupted = true;
            reviews_job.interrupted = context.include_reviews;
            review_threads_job.interrupted = context.include_review_threads;
            break;
        }
        if context.include_reviews {
            let reviews_result = sync_thread_reviews(
                archive,
                client,
                &family_scope,
                metadata_result.value.as_ref(),
                metadata_result.failure.as_ref(),
                context,
            )
            .await?;
            accumulate_thread_result(&mut reviews_job, &reviews_result)?;
            summary.reviews_seen = summary
                .reviews_seen
                .checked_add(reviews_result.items_received)
                .ok_or(StoreError::IntegerOutOfRange)?;
            if reviews_result.interrupted {
                metadata_job.interrupted = true;
                reviews_job.interrupted = true;
                review_threads_job.interrupted = context.include_review_threads;
                break;
            }
        }
        if context.include_review_threads {
            let review_threads_result = sync_thread_review_threads(
                archive,
                client,
                &family_scope,
                metadata_result.value.as_ref(),
                metadata_result.failure.as_ref(),
                context,
            )
            .await?;
            accumulate_thread_result(&mut review_threads_job, &review_threads_result)?;
            summary.review_threads_seen = summary
                .review_threads_seen
                .checked_add(review_threads_result.items_received)
                .ok_or(StoreError::IntegerOutOfRange)?;
            if review_threads_result.interrupted {
                metadata_job.interrupted = true;
                review_threads_job.interrupted = true;
                break;
            }
        }
    }

    summary.pages_completed = summary
        .pages_completed
        .checked_add(metadata_job.pages_completed)
        .and_then(|count| count.checked_add(reviews_job.pages_completed))
        .and_then(|count| count.checked_add(review_threads_job.pages_completed))
        .ok_or(StoreError::IntegerOutOfRange)?;
    finish_family_sync_job(
        archive,
        context,
        summary,
        &progress_repository,
        metadata_job_id,
        metadata_job,
    )
    .await?;
    if let Some(reviews_job_id) = reviews_job_id {
        finish_family_sync_job(
            archive,
            context,
            summary,
            &progress_repository,
            reviews_job_id,
            reviews_job,
        )
        .await?;
    }
    if let Some(review_threads_job_id) = review_threads_job_id {
        finish_family_sync_job(
            archive,
            context,
            summary,
            &progress_repository,
            review_threads_job_id,
            review_threads_job,
        )
        .await?;
    }
    Ok(())
}

async fn pull_request_targets(
    archive: &Archive,
    repository: &forgesync_core::content::Repository,
    unit: ScopeUnit,
) -> Result<Vec<PullRequestTarget>, EngineError> {
    let page_limit = NonZeroU32::new(1000).ok_or(EngineError::InvalidPageLimit)?;
    let mut offset = 0_u64;
    let mut targets = Vec::new();
    loop {
        let page = archive
            .query_threads(&ThreadQuery {
                repositories: vec![repository.id.clone()],
                kind: Some(ThreadKind::PullRequest),
                state: store_state_filter(unit.state),
                match_expression: None,
                updated_since: None,
                sort: ThreadSort::Updated,
                limit: page_limit,
                offset,
            })
            .await?;
        targets.extend(page.items.into_iter().map(|summary| PullRequestTarget {
            thread: summary.discussion.id,
            updated_at: summary.discussion.updated_at,
        }));
        let Some(next_offset) = page.next_offset else {
            break;
        };
        offset = next_offset;
    }
    Ok(targets)
}

fn accumulate_thread_result<T>(
    job: &mut FamilyJobAccumulator,
    result: &ThreadFamilyResult<T>,
) -> Result<(), StoreError> {
    job.pages_completed = job
        .pages_completed
        .checked_add(result.pages_completed)
        .ok_or(StoreError::IntegerOutOfRange)?;
    job.items_committed = job
        .items_committed
        .checked_add(result.items_committed)
        .ok_or(StoreError::IntegerOutOfRange)?;
    job.interrupted |= result.interrupted;
    if let Some(failure) = result.failure.as_ref() {
        let failure_slot = if failure.kind == FailureKind::RateLimited {
            &mut job.deferred_failure
        } else {
            &mut job.hard_failure
        };
        if failure_slot.is_none() {
            *failure_slot = Some(failure.clone());
        }
    }
    Ok(())
}

async fn finish_family_sync_job(
    archive: &Archive,
    context: &SyncRunContext<'_>,
    summary: &mut WorkSummary,
    repository: &str,
    job_id: i64,
    job: FamilyJobAccumulator,
) -> Result<(), EngineError> {
    let (status, failure, progress_status) = if job.interrupted {
        (
            SyncJobStatus::Interrupted,
            None,
            SyncProgressStatus::Interrupted,
        )
    } else if let Some(failure) = job.hard_failure {
        (
            SyncJobStatus::Failed,
            Some(failure.clone()),
            progress_status(&failure),
        )
    } else if let Some(failure) = job.deferred_failure {
        (
            SyncJobStatus::Deferred,
            Some(failure.clone()),
            progress_status(&failure),
        )
    } else {
        (SyncJobStatus::Complete, None, SyncProgressStatus::Complete)
    };
    archive
        .finish_sync_job(
            context.lease,
            job_id,
            SyncJobCompletion {
                status,
                updated_at: now_utc()?,
                pages_completed: job.pages_completed,
                items_committed: job.items_committed,
                failure: failure.as_ref(),
            },
        )
        .await?;
    summary.completed_jobs = summary.completed_jobs.saturating_add(1);
    if let Some(failure) = failure.as_ref() {
        count_failure(summary, failure);
    }
    if job.interrupted {
        summary.interrupted = true;
        summary.interrupted_jobs = summary.interrupted_jobs.saturating_add(1);
    }
    send_progress(
        &context.progress,
        context.run_id,
        summary,
        summary.total_jobs,
        Some(repository.to_owned()),
        progress_status,
    );
    Ok(())
}

async fn run_comment_job(
    archive: &Archive,
    client: &GitHubClient,
    repository: &forgesync_core::content::Repository,
    unit: ScopeUnit,
    context: &SyncRunContext<'_>,
    summary: &mut WorkSummary,
) -> Result<(), EngineError> {
    let started_at = now_utc()?;
    let job_id = archive
        .start_sync_job(
            context.lease,
            context.run_id,
            &repository.id,
            EvidenceFamily::Comments,
            unit.key,
            started_at,
        )
        .await?;
    let selector_target = RepositorySelector::from_repository(repository).as_url();
    archive
        .mark_scope_failures_retried(
            context.lease,
            &RunFailureScope {
                run_id: context.run_id,
                target: &selector_target,
                family: EvidenceFamily::Comments,
                scope_key: unit.key,
            },
        )
        .await?;
    let progress_repository = RepositorySelector::from_repository(repository).as_url();
    send_progress(
        &context.progress,
        context.run_id,
        summary,
        summary.total_jobs,
        Some(progress_repository.clone()),
        SyncProgressStatus::InProgress,
    );

    let mut pages_completed = 0_u64;
    let mut comments_seen = 0_u64;
    let mut items_committed = 0_u64;
    let mut first_hard_failure = None;
    let mut first_deferred_failure = None;
    let mut interrupted = false;
    let page_limit = NonZeroU32::new(1000).ok_or(EngineError::InvalidPageLimit)?;
    let mut offset = 0_u64;

    loop {
        if context.cancellation.is_cancelled() {
            interrupted = true;
            break;
        }
        let thread_page = archive
            .query_threads(&ThreadQuery {
                repositories: vec![repository.id.clone()],
                kind: None,
                state: store_state_filter(unit.state),
                match_expression: None,
                updated_since: None,
                sort: ThreadSort::Updated,
                limit: page_limit,
                offset,
            })
            .await?;

        for thread in thread_page.items {
            if context.cancellation.is_cancelled() {
                interrupted = true;
                break;
            }
            let result = sync_thread_comments(
                archive,
                client,
                &thread.repository,
                &thread.discussion,
                unit.key,
                context,
            )
            .await?;
            pages_completed = pages_completed
                .checked_add(result.pages_completed)
                .ok_or(StoreError::IntegerOutOfRange)?;
            comments_seen = comments_seen
                .checked_add(result.comments_received)
                .ok_or(StoreError::IntegerOutOfRange)?;
            items_committed = items_committed
                .checked_add(result.comments_committed)
                .ok_or(StoreError::IntegerOutOfRange)?;
            if let Some(failure) = result.failure {
                if failure.kind == FailureKind::RateLimited {
                    if first_deferred_failure.is_none() {
                        first_deferred_failure = Some(failure);
                    }
                } else if first_hard_failure.is_none() {
                    first_hard_failure = Some(failure);
                }
            }
            if result.interrupted {
                interrupted = true;
                break;
            }
        }
        if interrupted {
            break;
        }
        let Some(next_offset) = thread_page.next_offset else {
            break;
        };
        offset = next_offset;
    }

    summary.pages_completed = summary
        .pages_completed
        .checked_add(pages_completed)
        .ok_or(StoreError::IntegerOutOfRange)?;
    summary.comments_seen = summary
        .comments_seen
        .checked_add(comments_seen)
        .ok_or(StoreError::IntegerOutOfRange)?;

    let (status, failure) = if interrupted {
        (SyncJobStatus::Interrupted, None)
    } else if let Some(failure) = first_hard_failure {
        (SyncJobStatus::Failed, Some(failure))
    } else if let Some(failure) = first_deferred_failure {
        (SyncJobStatus::Deferred, Some(failure))
    } else {
        (SyncJobStatus::Complete, None)
    };
    archive
        .finish_sync_job(
            context.lease,
            job_id,
            SyncJobCompletion {
                status,
                updated_at: now_utc()?,
                pages_completed,
                items_committed,
                // Thread-specific failures already have their own durable records.
                failure: None,
            },
        )
        .await?;
    if status == SyncJobStatus::Complete {
        archive
            .resolve_scope_failures(
                context.lease,
                &RunFailureScope {
                    run_id: context.run_id,
                    target: &selector_target,
                    family: EvidenceFamily::Comments,
                    scope_key: unit.key,
                },
                now_utc()?,
            )
            .await?;
    }
    summary.completed_jobs += 1;
    if let Some(failure) = failure.as_ref() {
        count_failure(summary, failure);
    }
    if interrupted {
        summary.interrupted = true;
        summary.interrupted_jobs += 1;
    }
    send_progress(
        &context.progress,
        context.run_id,
        summary,
        summary.total_jobs,
        Some(progress_repository),
        if interrupted {
            SyncProgressStatus::Interrupted
        } else if let Some(failure) = failure.as_ref() {
            progress_status(failure)
        } else {
            SyncProgressStatus::Complete
        },
    );
    Ok(())
}

async fn sync_thread_pull_request_metadata(
    archive: &Archive,
    client: &GitHubClient,
    scope: &ThreadFamilyScope<'_>,
    context: &SyncRunContext<'_>,
) -> Result<ThreadFamilyResult<PullRequestMetadata>, EngineError> {
    let mut result = ThreadFamilyResult::default();
    let family = EvidenceFamily::PullRequestMetadata;
    let failure_scope = ChildFamilyFailureScope {
        run_id: context.run_id,
        repository: &scope.repository.id,
        thread: scope.thread,
        family,
        scope_key: scope.key,
    };
    let source_clock = SourceClock::Valid(scope.updated_at);
    let request_scope = format!("run:{}:{}", context.run_id.get(), scope.key);
    let reservation = archive
        .reserve_child_family_observation_fenced(
            scope.thread,
            family,
            &source_clock,
            now_utc()?,
            &request_scope,
            context.lease,
        )
        .await?;
    if !reservation.reserved {
        return Err(StoreError::StaleObservationGeneration.into());
    }
    archive
        .mark_child_family_failures_retried(context.lease, &failure_scope)
        .await?;

    let metadata = match fetch_pull_request_metadata(
        client,
        scope.repository,
        scope.thread,
        context.cancellation,
    )
    .await
    {
        Ok(metadata) => metadata,
        Err(error) => {
            archive
                .finish_child_family_observation_fenced(
                    ChildFamilyObservation {
                        thread: scope.thread,
                        family,
                        sequence: reservation.sequence,
                        observed_at: now_utc()?,
                        completeness: &CollectionCompleteness::Incomplete {
                            reason: incomplete_reason(&error, 0),
                            received_items: 0,
                        },
                        expected_pages: None,
                        head_sha: None,
                    },
                    context.lease,
                )
                .await?;
            if matches!(error, GitHubError::Cancelled) {
                result.interrupted = true;
                return Ok(result);
            }
            let failure = github_failure(&error);
            record_thread_family_failure(
                archive,
                context,
                scope.repository,
                scope.thread,
                family,
                scope.key,
                &failure,
            )
            .await?;
            result.failure = Some(failure);
            return Ok(result);
        }
    };

    let item = StagedItem {
        id: scope.thread.provider_id().clone(),
        payload: metadata.clone(),
    };
    archive
        .stage_child_family_page_fenced(
            scope.thread,
            family,
            reservation.sequence,
            0,
            &[item],
            context.lease,
        )
        .await?;
    let observation = archive
        .finish_child_family_observation_fenced(
            ChildFamilyObservation {
                thread: scope.thread,
                family,
                sequence: reservation.sequence,
                observed_at: now_utc()?,
                completeness: &CollectionCompleteness::Complete,
                expected_pages: Some(1),
                head_sha: None,
            },
            context.lease,
        )
        .await?;
    if matches!(
        observation.disposition,
        ObservationDisposition::Applied | ObservationDisposition::Replayed
    ) {
        result.pages_completed = 1;
        result.items_received = 1;
        result.items_committed = observation.item_count;
        result.value = Some(metadata);
        archive
            .resolve_child_family_failures(context.lease, &failure_scope, now_utc()?)
            .await?;
    } else {
        return Err(StoreError::StaleObservationGeneration.into());
    }
    Ok(result)
}

async fn sync_thread_reviews(
    archive: &Archive,
    client: &GitHubClient,
    scope: &ThreadFamilyScope<'_>,
    metadata: Option<&PullRequestMetadata>,
    metadata_failure: Option<&Failure>,
    context: &SyncRunContext<'_>,
) -> Result<ThreadFamilyResult<()>, EngineError> {
    let mut result = ThreadFamilyResult::default();
    let family = EvidenceFamily::Reviews;
    let failure_scope = ChildFamilyFailureScope {
        run_id: context.run_id,
        repository: &scope.repository.id,
        thread: scope.thread,
        family,
        scope_key: scope.key,
    };
    let source_clock = SourceClock::Valid(scope.updated_at);
    if let Some(metadata) = metadata
        && archive
            .pull_request_family_is_current_for_head(
                scope.thread,
                family,
                &source_clock,
                &metadata.head.sha,
            )
            .await?
    {
        archive
            .resolve_child_family_failures(context.lease, &failure_scope, now_utc()?)
            .await?;
        return Ok(result);
    }

    let request_scope = format!("run:{}:{}", context.run_id.get(), scope.key);
    let reservation = archive
        .reserve_child_family_observation_fenced(
            scope.thread,
            family,
            &source_clock,
            now_utc()?,
            &request_scope,
            context.lease,
        )
        .await?;
    if !reservation.reserved {
        return Err(StoreError::StaleObservationGeneration.into());
    }
    archive
        .mark_child_family_failures_retried(context.lease, &failure_scope)
        .await?;

    let Some(metadata) = metadata else {
        let failure = metadata_failure.cloned().unwrap_or(Failure {
            kind: FailureKind::ProviderResponse,
            message: "pull-request head metadata is unavailable".to_owned(),
        });
        archive
            .finish_child_family_observation_fenced(
                ChildFamilyObservation {
                    thread: scope.thread,
                    family,
                    sequence: reservation.sequence,
                    observed_at: now_utc()?,
                    completeness: &CollectionCompleteness::Incomplete {
                        reason: IncompleteReason::Unknown,
                        received_items: 0,
                    },
                    expected_pages: None,
                    head_sha: None,
                },
                context.lease,
            )
            .await?;
        record_thread_family_failure(
            archive,
            context,
            scope.repository,
            scope.thread,
            family,
            scope.key,
            &failure,
        )
        .await?;
        result.failure = Some(failure);
        return Ok(result);
    };

    let mut next_page = None;
    let mut page_count = 0_u32;
    loop {
        let page = match fetch_pull_request_review_page(
            client,
            scope.repository,
            scope.thread,
            next_page.as_ref(),
            context.cancellation,
        )
        .await
        {
            Ok(page) => page,
            Err(error) => {
                archive
                    .finish_child_family_observation_fenced(
                        ChildFamilyObservation {
                            thread: scope.thread,
                            family,
                            sequence: reservation.sequence,
                            observed_at: now_utc()?,
                            completeness: &CollectionCompleteness::Incomplete {
                                reason: incomplete_reason(&error, page_count),
                                received_items: result.items_received,
                            },
                            expected_pages: None,
                            head_sha: None,
                        },
                        context.lease,
                    )
                    .await?;
                if matches!(error, GitHubError::Cancelled) {
                    result.interrupted = true;
                    return Ok(result);
                }
                let failure = github_failure(&error);
                record_thread_family_failure(
                    archive,
                    context,
                    scope.repository,
                    scope.thread,
                    family,
                    scope.key,
                    &failure,
                )
                .await?;
                result.failure = Some(failure);
                result.pages_completed = u64::from(page_count);
                return Ok(result);
            }
        };
        let page_items =
            u64::try_from(page.reviews.len()).map_err(|_| StoreError::IntegerOutOfRange)?;
        result.items_received = result
            .items_received
            .checked_add(page_items)
            .ok_or(StoreError::IntegerOutOfRange)?;
        let items = page
            .reviews
            .into_iter()
            .map(|review| StagedItem {
                id: review.id.provider_id().clone(),
                payload: review,
            })
            .collect::<Vec<StagedItem<Review>>>();
        archive
            .stage_child_family_page_fenced(
                scope.thread,
                family,
                reservation.sequence,
                page_count,
                &items,
                context.lease,
            )
            .await?;
        page_count = page_count
            .checked_add(1)
            .ok_or(StoreError::IntegerOutOfRange)?;
        next_page = page.next_page;
        if next_page.is_none() {
            break;
        }
    }

    let observation = archive
        .finish_child_family_observation_fenced(
            ChildFamilyObservation {
                thread: scope.thread,
                family,
                sequence: reservation.sequence,
                observed_at: now_utc()?,
                completeness: &CollectionCompleteness::Complete,
                expected_pages: Some(page_count),
                head_sha: Some(&metadata.head.sha),
            },
            context.lease,
        )
        .await?;
    result.pages_completed = u64::from(page_count);
    if matches!(
        observation.disposition,
        ObservationDisposition::Applied | ObservationDisposition::Replayed
    ) {
        result.items_committed = observation.item_count;
        archive
            .resolve_child_family_failures(context.lease, &failure_scope, now_utc()?)
            .await?;
    } else {
        return Err(StoreError::StaleObservationGeneration.into());
    }
    Ok(result)
}

async fn sync_thread_review_threads(
    archive: &Archive,
    client: &GitHubClient,
    scope: &ThreadFamilyScope<'_>,
    metadata: Option<&PullRequestMetadata>,
    metadata_failure: Option<&Failure>,
    context: &SyncRunContext<'_>,
) -> Result<ThreadFamilyResult<()>, EngineError> {
    let mut result = ThreadFamilyResult::default();
    let family = EvidenceFamily::ReviewThreads;
    let failure_scope = ChildFamilyFailureScope {
        run_id: context.run_id,
        repository: &scope.repository.id,
        thread: scope.thread,
        family,
        scope_key: scope.key,
    };
    let source_clock = SourceClock::Valid(scope.updated_at);
    if let Some(metadata) = metadata
        && archive
            .pull_request_family_is_current_for_head(
                scope.thread,
                family,
                &source_clock,
                &metadata.head.sha,
            )
            .await?
    {
        archive
            .resolve_child_family_failures(context.lease, &failure_scope, now_utc()?)
            .await?;
        return Ok(result);
    }

    let request_scope = format!("run:{}:{}", context.run_id.get(), scope.key);
    let reservation = archive
        .reserve_child_family_observation_fenced(
            scope.thread,
            family,
            &source_clock,
            now_utc()?,
            &request_scope,
            context.lease,
        )
        .await?;
    if !reservation.reserved {
        return Err(StoreError::StaleObservationGeneration.into());
    }
    archive
        .mark_child_family_failures_retried(context.lease, &failure_scope)
        .await?;

    let Some(metadata) = metadata else {
        let failure = metadata_failure.cloned().unwrap_or(Failure {
            kind: FailureKind::ProviderResponse,
            message: "pull-request head metadata is unavailable".to_owned(),
        });
        archive
            .finish_child_family_observation_fenced(
                ChildFamilyObservation {
                    thread: scope.thread,
                    family,
                    sequence: reservation.sequence,
                    observed_at: now_utc()?,
                    completeness: &CollectionCompleteness::Incomplete {
                        reason: IncompleteReason::Unknown,
                        received_items: 0,
                    },
                    expected_pages: None,
                    head_sha: None,
                },
                context.lease,
            )
            .await?;
        record_thread_family_failure(
            archive,
            context,
            scope.repository,
            scope.thread,
            family,
            scope.key,
            &failure,
        )
        .await?;
        result.failure = Some(failure);
        return Ok(result);
    };

    let mut next_page: Option<GraphqlCursor> = None;
    let mut seen_cursors = HashSet::new();
    let mut page_count = 0_u32;
    loop {
        let page = match fetch_review_thread_page(
            client,
            scope.repository,
            scope.thread,
            &metadata.head.sha,
            next_page.as_ref(),
            context.cancellation,
        )
        .await
        {
            Ok(page) => page,
            Err(error) => {
                archive
                    .finish_child_family_observation_fenced(
                        ChildFamilyObservation {
                            thread: scope.thread,
                            family,
                            sequence: reservation.sequence,
                            observed_at: now_utc()?,
                            completeness: &CollectionCompleteness::Incomplete {
                                reason: incomplete_reason(&error, page_count),
                                received_items: result.items_received,
                            },
                            expected_pages: None,
                            head_sha: None,
                        },
                        context.lease,
                    )
                    .await?;
                if matches!(error, GitHubError::Cancelled) {
                    result.interrupted = true;
                    return Ok(result);
                }
                let failure = github_failure(&error);
                record_thread_family_failure(
                    archive,
                    context,
                    scope.repository,
                    scope.thread,
                    family,
                    scope.key,
                    &failure,
                )
                .await?;
                result.failure = Some(failure);
                result.pages_completed = u64::from(page_count);
                return Ok(result);
            }
        };

        if let Some(cursor) = page.next_cursor.as_ref()
            && !seen_cursors.insert(cursor.as_str().to_owned())
        {
            let error = GitHubError::InvalidPaginationLink;
            archive
                .finish_child_family_observation_fenced(
                    ChildFamilyObservation {
                        thread: scope.thread,
                        family,
                        sequence: reservation.sequence,
                        observed_at: now_utc()?,
                        completeness: &CollectionCompleteness::Incomplete {
                            reason: incomplete_reason(&error, page_count),
                            received_items: result.items_received,
                        },
                        expected_pages: None,
                        head_sha: None,
                    },
                    context.lease,
                )
                .await?;
            let failure = github_failure(&error);
            record_thread_family_failure(
                archive,
                context,
                scope.repository,
                scope.thread,
                family,
                scope.key,
                &failure,
            )
            .await?;
            result.failure = Some(failure);
            result.pages_completed = u64::from(page_count);
            return Ok(result);
        }

        let page_items =
            u64::try_from(page.review_threads.len()).map_err(|_| StoreError::IntegerOutOfRange)?;
        result.items_received = result
            .items_received
            .checked_add(page_items)
            .ok_or(StoreError::IntegerOutOfRange)?;
        let items = page
            .review_threads
            .into_iter()
            .map(|review_thread| StagedItem {
                id: review_thread.id.provider_id().clone(),
                payload: review_thread,
            })
            .collect::<Vec<StagedItem<ReviewThread>>>();
        archive
            .stage_child_family_page_fenced(
                scope.thread,
                family,
                reservation.sequence,
                page_count,
                &items,
                context.lease,
            )
            .await?;
        page_count = page_count
            .checked_add(1)
            .ok_or(StoreError::IntegerOutOfRange)?;
        next_page = page.next_cursor;
        if next_page.is_none() {
            break;
        }
    }

    let observation = archive
        .finish_child_family_observation_fenced(
            ChildFamilyObservation {
                thread: scope.thread,
                family,
                sequence: reservation.sequence,
                observed_at: now_utc()?,
                completeness: &CollectionCompleteness::Complete,
                expected_pages: Some(page_count),
                head_sha: Some(&metadata.head.sha),
            },
            context.lease,
        )
        .await?;
    result.pages_completed = u64::from(page_count);
    if matches!(
        observation.disposition,
        ObservationDisposition::Applied | ObservationDisposition::Replayed
    ) {
        result.items_committed = observation.item_count;
        archive
            .resolve_child_family_failures(context.lease, &failure_scope, now_utc()?)
            .await?;
    } else {
        return Err(StoreError::StaleObservationGeneration.into());
    }
    Ok(result)
}

async fn record_thread_family_failure(
    archive: &Archive,
    context: &SyncRunContext<'_>,
    repository: &forgesync_core::content::Repository,
    thread: &ThreadId,
    family: EvidenceFamily,
    scope_key: &str,
    failure: &Failure,
) -> Result<(), EngineError> {
    archive
        .record_run_failure(
            context.lease,
            RunFailureInput {
                run_id: context.run_id,
                target: &repository.full_name,
                repository: Some(&repository.id),
                thread: Some(thread),
                family: Some(family),
                scope_key,
                failure,
                created_at: now_utc()?,
            },
        )
        .await
        .map_err(|source| EngineError::FailureLedger {
            original: failure.clone(),
            source,
        })?;
    Ok(())
}

fn incomplete_reason(error: &GitHubError, pages_completed: u32) -> IncompleteReason {
    if matches!(error, GitHubError::Cancelled) {
        IncompleteReason::Cancelled
    } else if matches!(error, GitHubError::Deferred { .. }) {
        IncompleteReason::RetryBudget
    } else if pages_completed > 0 {
        IncompleteReason::Pagination
    } else {
        IncompleteReason::Unknown
    }
}

async fn sync_thread_comments(
    archive: &Archive,
    client: &GitHubClient,
    repository: &forgesync_core::content::Repository,
    discussion: &forgesync_core::content::Discussion,
    scope_key: &str,
    context: &SyncRunContext<'_>,
) -> Result<CommentThreadResult, EngineError> {
    let failure_scope = ChildFamilyFailureScope {
        run_id: context.run_id,
        repository: &repository.id,
        thread: &discussion.id,
        family: EvidenceFamily::Comments,
        scope_key,
    };
    let source_clock = SourceClock::Valid(discussion.updated_at);
    let expected_item_count = comment_count(discussion);
    if archive
        .child_family_is_current(
            &discussion.id,
            EvidenceFamily::Comments,
            &source_clock,
            expected_item_count,
        )
        .await?
    {
        archive
            .resolve_child_family_failures(context.lease, &failure_scope, now_utc()?)
            .await?;
        return Ok(CommentThreadResult::default());
    }

    let started_at = now_utc()?;
    let request_scope = format!("run:{}:{}", context.run_id.get(), scope_key);
    let reservation = archive
        .reserve_child_family_observation_fenced(
            &discussion.id,
            EvidenceFamily::Comments,
            &source_clock,
            started_at,
            &request_scope,
            context.lease,
        )
        .await?;
    if !reservation.reserved {
        return Err(StoreError::StaleObservationGeneration.into());
    }
    archive
        .mark_child_family_failures_retried(context.lease, &failure_scope)
        .await?;

    let mut result = CommentThreadResult::default();
    let mut next_page = None;
    let mut page_count = 0_u32;
    loop {
        let page = match fetch_issue_comment_page(
            client,
            repository,
            &discussion.id,
            next_page.as_ref(),
            context.cancellation,
        )
        .await
        {
            Ok(page) => page,
            Err(error) => {
                result.pages_completed = u64::from(page_count);
                let incomplete_reason = if matches!(error, GitHubError::Cancelled) {
                    forgesync_core::observation::IncompleteReason::Cancelled
                } else if matches!(error, GitHubError::Deferred { .. }) {
                    forgesync_core::observation::IncompleteReason::RetryBudget
                } else if page_count > 0 {
                    forgesync_core::observation::IncompleteReason::Pagination
                } else {
                    forgesync_core::observation::IncompleteReason::Unknown
                };
                let failure = github_failure(&error);
                archive
                    .finish_child_family_observation_fenced(
                        ChildFamilyObservation {
                            thread: &discussion.id,
                            family: EvidenceFamily::Comments,
                            sequence: reservation.sequence,
                            observed_at: now_utc()?,
                            completeness: &CollectionCompleteness::Incomplete {
                                reason: incomplete_reason,
                                received_items: result.comments_received,
                            },
                            expected_pages: None,
                            head_sha: None,
                        },
                        context.lease,
                    )
                    .await?;
                if matches!(error, GitHubError::Cancelled) {
                    result.interrupted = true;
                    return Ok(result);
                }
                archive
                    .record_run_failure(
                        context.lease,
                        RunFailureInput {
                            run_id: context.run_id,
                            target: &repository.full_name,
                            repository: Some(&repository.id),
                            thread: Some(&discussion.id),
                            family: Some(EvidenceFamily::Comments),
                            scope_key,
                            failure: &failure,
                            created_at: now_utc()?,
                        },
                    )
                    .await
                    .map_err(|source| EngineError::FailureLedger {
                        original: failure.clone(),
                        source,
                    })?;
                result.failure = Some(failure);
                return Ok(result);
            }
        };

        let item_count =
            u64::try_from(page.comments.len()).map_err(|_| StoreError::IntegerOutOfRange)?;
        result.comments_received = result
            .comments_received
            .checked_add(item_count)
            .ok_or(StoreError::IntegerOutOfRange)?;
        let items = page
            .comments
            .into_iter()
            .map(|comment| StagedItem {
                id: comment.id.provider_id().clone(),
                payload: comment,
            })
            .collect::<Vec<StagedItem<Comment>>>();
        archive
            .stage_child_family_page_fenced(
                &discussion.id,
                EvidenceFamily::Comments,
                reservation.sequence,
                page_count,
                &items,
                context.lease,
            )
            .await?;
        page_count = page_count
            .checked_add(1)
            .ok_or(StoreError::IntegerOutOfRange)?;
        next_page = page.next_page;
        if next_page.is_none() {
            break;
        }
    }

    let observation = archive
        .finish_child_family_observation_fenced(
            ChildFamilyObservation {
                thread: &discussion.id,
                family: EvidenceFamily::Comments,
                sequence: reservation.sequence,
                observed_at: now_utc()?,
                completeness: &CollectionCompleteness::Complete,
                expected_pages: Some(page_count),
                head_sha: None,
            },
            context.lease,
        )
        .await?;
    result.pages_completed = u64::from(page_count);
    if matches!(
        observation.disposition,
        ObservationDisposition::Applied | ObservationDisposition::Replayed
    ) {
        result.comments_committed = observation.item_count;
        archive
            .resolve_child_family_failures(context.lease, &failure_scope, now_utc()?)
            .await?;
    }
    Ok(result)
}

fn comment_count(discussion: &forgesync_core::content::Discussion) -> Option<u64> {
    discussion
        .provider_data
        .get("comments")
        .and_then(serde_json::Value::as_u64)
}

fn store_state_filter(state: ThreadListState) -> ThreadStateFilter {
    match state {
        ThreadListState::All => ThreadStateFilter::All,
        ThreadListState::Open => ThreadStateFilter::Open,
        ThreadListState::Closed => ThreadStateFilter::Closed,
    }
}

async fn resolve_selectors(
    archive: &Archive,
    request: &SyncRequest,
) -> Result<Vec<RepositorySelector>, EngineError> {
    if request.all {
        Ok(archive
            .list_repositories()
            .await?
            .iter()
            .map(RepositorySelector::from_repository)
            .collect())
    } else {
        Ok(request.repositories.clone())
    }
}

fn job_result(
    report: &ThreadEnumerationReport,
) -> (SyncJobStatus, Option<Failure>, SyncProgressStatus) {
    if report.scan.status == RepositoryThreadScanStatus::Complete {
        return (SyncJobStatus::Complete, None, SyncProgressStatus::Complete);
    }
    if report.interrupted {
        return (
            SyncJobStatus::Interrupted,
            report.scan.failure.clone(),
            SyncProgressStatus::Interrupted,
        );
    }
    let failure = report.scan.failure.clone().unwrap_or(Failure {
        kind: FailureKind::ProviderResponse,
        message: "GitHub thread enumeration did not complete".to_owned(),
    });
    let status = if failure.kind == FailureKind::RateLimited {
        SyncJobStatus::Deferred
    } else {
        SyncJobStatus::Failed
    };
    (status, Some(failure.clone()), progress_status(&failure))
}

fn progress_status(failure: &Failure) -> SyncProgressStatus {
    if failure.kind == FailureKind::RateLimited {
        SyncProgressStatus::Deferred
    } else {
        SyncProgressStatus::Failed
    }
}

fn count_failure(summary: &mut WorkSummary, failure: &Failure) {
    if failure.kind == FailureKind::RateLimited {
        summary.deferred_jobs += 1;
    } else {
        summary.failed_jobs += 1;
    }
    if summary.first_failure.is_none() {
        summary.first_failure = Some(failure.clone());
    }
}

fn operation_outcome(work: &WorkSummary) -> OperationOutcome {
    if work.interrupted {
        return OperationOutcome::Interrupted {
            pending_items: work.pending_jobs,
        };
    }
    if work.failed_jobs > 0 || work.deferred_jobs > 0 {
        if work.completed_jobs > work.failed_jobs + work.deferred_jobs {
            return OperationOutcome::Partial {
                failed_items: work.failed_jobs,
                deferred_items: work.deferred_jobs,
            };
        }
        if work.failed_jobs == 0 {
            return OperationOutcome::Deferred {
                reason: DeferredReason::RateLimitBudget,
            };
        }
        return OperationOutcome::Failed {
            failure: work.first_failure.clone().unwrap_or(Failure {
                kind: FailureKind::ProviderResponse,
                message: "sync failed before any repository completed".to_owned(),
            }),
        };
    }
    OperationOutcome::Complete
}

fn overlap_start(watermark: UtcTimestamp) -> UtcTimestamp {
    UtcTimestamp::from_unix_microseconds(
        watermark
            .unix_microseconds()
            .saturating_sub(CLOSED_SWEEP_OVERLAP_MICROSECONDS),
    )
    .unwrap_or(watermark)
}

fn send_progress(
    sender: &Option<mpsc::Sender<SyncProgress>>,
    run_id: RunId,
    summary: &WorkSummary,
    total_jobs: u64,
    repository: Option<String>,
    status: SyncProgressStatus,
) {
    if let Some(sender) = sender {
        let event = SyncProgress {
            run_id,
            completed_jobs: summary.completed_jobs,
            total_jobs,
            threads_seen: summary.threads_seen,
            comments_seen: summary.comments_seen,
            pull_request_metadata_seen: summary.pull_request_metadata_seen,
            reviews_seen: summary.reviews_seen,
            review_threads_seen: summary.review_threads_seen,
            repository,
            status,
        };
        let _ = sender.try_send(event);
    }
}
