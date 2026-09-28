use std::collections::{HashMap, HashSet};
use std::time::Duration;

use forgesync_core::{
    DeferredReason, EvidenceFamily, Failure, FailureKind, GitHubHost, OperationOutcome, RunId,
    UtcTimestamp,
};
use forgesync_github::{GitHubClient, GitHubError, ThreadListState, fetch_repository};
use forgesync_store::{
    Archive, ArchiveLeaseToken, RepositoryThreadScanStatus, RunFailureInput, RunRecord, StoreError,
    SyncJobCompletion, SyncJobRecord, SyncJobStatus,
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
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
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
    /// Number of fully committed REST pages.
    pub pages_completed: u64,
    /// Number of discussion rows returned by committed pages.
    pub threads_seen: u64,
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
    completed_jobs: u64,
    failed_jobs: u64,
    deferred_jobs: u64,
    pages_completed: u64,
    threads_seen: u64,
    interrupted: bool,
    interrupted_jobs: u64,
    pending_jobs: u64,
    first_failure: Option<Failure>,
}

struct SyncRunContext<'a> {
    total_jobs: u64,
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
    });
    let run_id = match archive
        .create_run(&lease, None, started_at, &run_scope)
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
        total_jobs: context.total_jobs,
        failed_jobs: work.failed_jobs,
        deferred_jobs: work.deferred_jobs,
        pages_completed: work.pages_completed,
        threads_seen: work.threads_seen,
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
        completed_jobs: 0,
        failed_jobs: 0,
        deferred_jobs: 0,
        pages_completed: 0,
        threads_seen: 0,
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
                    archive
                        .record_run_failure(
                            context.lease,
                            RunFailureInput {
                                run_id: context.run_id,
                                target: &selector.as_url(),
                                family: Some(EvidenceFamily::Threads),
                                scope_key: unit.key,
                                failure: &failure,
                                created_at: now_utc()?,
                            },
                        )
                        .await?;
                    count_failure(&mut summary, &failure);
                    summary.completed_jobs += 1;
                    send_progress(
                        &context.progress,
                        context.run_id,
                        &summary,
                        context.total_jobs,
                        Some(selector.as_url()),
                        progress_status(&failure),
                    );
                }
                continue;
            }
        };
        archive
            .upsert_repository_fenced(&repository, context.lease)
            .await?;

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
            send_progress(
                &context.progress,
                context.run_id,
                &summary,
                context.total_jobs,
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
                context.total_jobs,
                Some(selector.as_url()),
                progress_status,
            );
            if summary.interrupted {
                break;
            }
        }
        if summary.interrupted {
            break;
        }
    }
    if summary.interrupted {
        summary.pending_jobs = context
            .total_jobs
            .saturating_sub(summary.completed_jobs)
            .saturating_add(summary.interrupted_jobs);
    }
    Ok(summary)
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
            repository,
            status,
        };
        let _ = sender.try_send(event);
    }
}
