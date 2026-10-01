//! One repository's durable jobs: lookup, parent scans, comments, and pull-request families.
//!
//! For each thread-state unit the parent scan runs first, then comments, then pull-request
//! metadata followed by its head-bound review families. Each job is started in the ledger before
//! acquisition and finished through the same terminal path.

use std::num::NonZeroU32;

use forgesync_core::content::{Repository, ThreadKind};
use forgesync_core::coverage::{EvidenceFamily, Failure, FailureKind};
use forgesync_core::identity::{ObservationSequence, ThreadId};
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_github::error::GitHubError;
use forgesync_github::resources::{ThreadListState, fetch_repository};
use forgesync_github::transport::GitHubClient;
use forgesync_store::checkpoints::ClosedSweepCheckpoint;
use forgesync_store::enumeration::RepositoryThreadScanStatus;
use forgesync_store::reads::{ThreadPage, ThreadQuery, ThreadSort, ThreadStateFilter};
use forgesync_store::runs::{
    RunFailureInput, RunFailureScope, SyncJobCompletion, SyncJobStart, SyncJobStatus,
};

use crate::clock::now_utc;
use crate::enumeration::{ThreadScanContext, enumerate_repository_thread_pages};
use crate::error::EngineError;
use crate::provider_failure::github_failure;
use crate::reference::RepositorySelector;
use crate::sync::accounting::{JobProgress, WorkSummary, progress_status};
use crate::sync::coordinator::Run;
use crate::sync::families::ThreadTarget;
use crate::sync::{SyncProgressStatus, SyncThreadScope};

/// Local thread-page size used to select discussions for child-family jobs.
const THREAD_PAGE: NonZeroU32 = NonZeroU32::new(1000).unwrap();
/// Replay overlap protecting closed-thread sweeps from timestamp boundary gaps.
const CLOSED_SWEEP_OVERLAP_MICROSECONDS: i64 = 86_400_000_000;

/// One durable thread-state enumeration within the selected sync scope.
///
/// Default sync expands into separate open and closed units so each has an independent ledger and
/// checkpoint. Only a complete scan of a watermark-updating unit advances the closed sweep.
#[derive(Clone, Copy)]
pub struct ScopeUnit {
    /// Stable ledger scope key, independent of provider pagination.
    pub key: &'static str,
    pub state: ThreadListState,
    /// Local filter matching `state`.
    filter: ThreadStateFilter,
    update_closed_watermark: bool,
}

impl ScopeUnit {
    const OPEN: Self = Self {
        key: "open",
        state: ThreadListState::Open,
        filter: ThreadStateFilter::Open,
        update_closed_watermark: false,
    };
    const CLOSED: Self = Self {
        key: "closed",
        state: ThreadListState::Closed,
        filter: ThreadStateFilter::Closed,
        update_closed_watermark: true,
    };
    const ALL: Self = Self {
        key: "all",
        state: ThreadListState::All,
        filter: ThreadStateFilter::All,
        update_closed_watermark: true,
    };

    /// Expands a requested thread scope into its independently checkpointed enumerations.
    pub fn expand(scope: SyncThreadScope) -> Vec<Self> {
        match scope {
            SyncThreadScope::Default => vec![Self::OPEN, Self::CLOSED],
            SyncThreadScope::Open => vec![Self::OPEN],
            SyncThreadScope::Closed => vec![Self::CLOSED],
            SyncThreadScope::All => vec![Self::ALL],
        }
    }
}

/// A resolved repository and the services used by its jobs.
pub struct RepositorySync<'a> {
    run: &'a Run<'a>,
    client: &'a GitHubClient,
    repository: Repository,
    /// Repository URL used for job failure targets and progress.
    target: String,
}

/// A started ledger job and the progress accumulated for its terminal write.
struct Job {
    id: i64,
    family: EvidenceFamily,
    progress: JobProgress,
}

impl<'a> RepositorySync<'a> {
    /// Resolves the repository once, then runs each unit's parent scan and child families.
    ///
    /// The first scan's acquisition order is reserved before repository lookup so the scan is
    /// ordered by when acquisition began, not when the lookup response arrived.
    pub async fn run(
        run: &'a Run<'a>,
        client: &'a GitHubClient,
        selector: &RepositorySelector,
        units: &[ScopeUnit],
        summary: &mut WorkSummary,
    ) -> Result<(), EngineError> {
        let mut reserved = Some(reserve(run).await?);
        let lookup = fetch_repository(
            client,
            selector.host(),
            selector.owner(),
            selector.name(),
            run.cancellation,
        )
        .await;
        let repository = match lookup {
            Ok(repository) => repository,
            Err(GitHubError::Cancelled) => {
                summary.interrupted = true;
                return Ok(());
            }
            Err(error) => return lookup_failed(run, selector, units, &error, summary).await,
        };
        run.archive
            .upsert_repository_fenced(&repository, run.lease)
            .await?;
        let sync = Self {
            run,
            client,
            target: RepositorySelector::from_repository(&repository).as_url(),
            repository,
        };
        for &unit in units {
            if run.cancellation.is_cancelled() {
                summary.interrupted = true;
                break;
            }
            let acquisition = match reserved.take() {
                Some(acquisition) => acquisition,
                None => reserve(run).await?,
            };
            sync.threads(unit, acquisition, summary).await?;
            if summary.interrupted {
                break;
            }
            if run.request.include_comments {
                sync.comments(unit, summary).await?;
            }
            if !summary.interrupted {
                sync.pull_requests(unit, summary).await?;
            }
            if summary.interrupted {
                break;
            }
        }
        Ok(())
    }

    /// Scans parent threads, then advances the closed watermark and resolves scope failures only
    /// after complete coverage.
    async fn threads(
        &self,
        unit: ScopeUnit,
        (started_at, sequence): (UtcTimestamp, ObservationSequence),
        summary: &mut WorkSummary,
    ) -> Result<(), EngineError> {
        let mut job = self
            .start_job(EvidenceFamily::Threads, unit, started_at, summary)
            .await?;
        let since = if unit.state == ThreadListState::Closed {
            self.run
                .archive
                .closed_sweep_watermark(&self.repository.id)
                .await?
                .map(overlap_start)
        } else {
            None
        };
        let scan = ThreadScanContext {
            repository: self.repository.clone(),
            sequence,
            started_at,
            state: unit.state,
            since,
        };
        let report = enumerate_repository_thread_pages(
            self.run.archive,
            self.client,
            scan,
            Some(self.run.lease),
            self.run.cancellation,
        )
        .await?;
        let complete = report.scan.status == RepositoryThreadScanStatus::Complete;
        job.progress = JobProgress {
            pages_completed: report.scan.pages_completed,
            items_received: report.scan.threads_seen,
            items_committed: report.scan.threads_seen,
            interrupted: !complete && report.interrupted,
            ..JobProgress::default()
        };
        if !complete && !report.interrupted {
            job.progress
                .add_failure(report.scan.failure.unwrap_or(Failure {
                    kind: FailureKind::ProviderResponse,
                    message: "GitHub thread enumeration did not complete".to_owned(),
                }));
        }
        if self.finish_job(job, summary).await? != SyncJobStatus::Complete {
            return Ok(());
        }
        if unit.update_closed_watermark {
            let checkpoint = ClosedSweepCheckpoint {
                repository: &self.repository.id,
                sequence,
                watermark: started_at,
                updated_at: now_utc()?,
            };
            self.run
                .archive
                .commit_closed_sweep_watermark(self.run.lease, checkpoint)
                .await?;
        }
        self.resolve_scope_failures(EvidenceFamily::Threads, unit)
            .await
    }

    /// Collects comments for every archived discussion in the unit.
    async fn comments(
        &self,
        unit: ScopeUnit,
        summary: &mut WorkSummary,
    ) -> Result<(), EngineError> {
        let mut job = self
            .start_job(EvidenceFamily::Comments, unit, now_utc()?, summary)
            .await?;
        let mut offset = 0;
        'pages: loop {
            if self.run.cancellation.is_cancelled() {
                job.progress.interrupted = true;
                break;
            }
            let page = self.thread_page(unit, None, offset).await?;
            for thread in page.items {
                if self.run.cancellation.is_cancelled() {
                    job.progress.interrupted = true;
                    break 'pages;
                }
                let result = self
                    .target(unit, &thread.discussion.id, thread.discussion.updated_at)
                    .comments(&thread.discussion)
                    .await?;
                job.progress.add(&result);
                if result.interrupted {
                    break 'pages;
                }
            }
            let Some(next) = page.next_offset else {
                break;
            };
            offset = next;
        }
        if self.finish_job(job, summary).await? == SyncJobStatus::Complete {
            self.resolve_scope_failures(EvidenceFamily::Comments, unit)
                .await?;
        }
        Ok(())
    }

    /// Acquires metadata, then the selected review families, for each archived pull request.
    ///
    /// Jobs exist only for units containing pull requests. Cancellation marks the jobs affected
    /// by the interrupted phase; a review-thread interruption leaves completed reviews intact.
    async fn pull_requests(
        &self,
        unit: ScopeUnit,
        summary: &mut WorkSummary,
    ) -> Result<(), EngineError> {
        let mut targets = Vec::new();
        let mut offset = 0;
        loop {
            let page = self
                .thread_page(unit, Some(ThreadKind::PullRequest), offset)
                .await?;
            targets.extend(
                page.items
                    .into_iter()
                    .map(|thread| (thread.discussion.id, thread.discussion.updated_at)),
            );
            let Some(next) = page.next_offset else {
                break;
            };
            offset = next;
        }
        if targets.is_empty() {
            return Ok(());
        }
        let request = self.run.request;
        summary.total_jobs +=
            1 + u64::from(request.include_reviews) + u64::from(request.include_review_threads);
        let started_at = now_utc()?;
        let mut metadata = self
            .start_job(
                EvidenceFamily::PullRequestMetadata,
                unit,
                started_at,
                summary,
            )
            .await?;
        let mut reviews = None;
        if request.include_reviews {
            let job = self.start_job(EvidenceFamily::Reviews, unit, started_at, summary);
            reviews = Some(job.await?);
        }
        let mut review_threads = None;
        if request.include_review_threads {
            let job = self.start_job(EvidenceFamily::ReviewThreads, unit, started_at, summary);
            review_threads = Some(job.await?);
        }
        for (thread, updated_at) in &targets {
            if self.run.cancellation.is_cancelled() {
                interrupt([
                    Some(&mut metadata),
                    reviews.as_mut(),
                    review_threads.as_mut(),
                ]);
                break;
            }
            let target = self.target(unit, thread, *updated_at);
            let head = target.metadata().await?;
            metadata.progress.add(&head.0);
            let mut interrupted = head.0.interrupted;
            if !interrupted && let Some(job) = &mut reviews {
                let result = target.review_family(job.family, &head).await?;
                job.progress.add(&result);
                interrupted = result.interrupted;
            }
            if interrupted {
                interrupt([
                    Some(&mut metadata),
                    reviews.as_mut(),
                    review_threads.as_mut(),
                ]);
                break;
            }
            if let Some(job) = &mut review_threads {
                let result = target.review_family(job.family, &head).await?;
                job.progress.add(&result);
                if result.interrupted {
                    metadata.progress.interrupted = true;
                    break;
                }
            }
        }
        for job in [Some(metadata), reviews, review_threads]
            .into_iter()
            .flatten()
        {
            self.finish_job(job, summary).await?;
        }
        Ok(())
    }

    /// Identifies one discussion's child-family acquisitions within `unit`.
    fn target<'s>(
        &'s self,
        unit: ScopeUnit,
        thread: &'s ThreadId,
        updated_at: UtcTimestamp,
    ) -> ThreadTarget<'s> {
        ThreadTarget {
            run: self.run,
            client: self.client,
            repository: &self.repository,
            scope_key: unit.key,
            thread,
            updated_at,
        }
    }

    /// Reads one bounded page of archived discussions selected by the unit.
    async fn thread_page(
        &self,
        unit: ScopeUnit,
        kind: Option<ThreadKind>,
        offset: u64,
    ) -> Result<ThreadPage, EngineError> {
        let query = ThreadQuery {
            repositories: vec![self.repository.id.clone()],
            kind,
            state: unit.filter,
            match_expression: None,
            updated_since: None,
            sort: ThreadSort::Updated,
            limit: THREAD_PAGE,
            offset,
        };
        Ok(self.run.archive.query_threads(&query).await?)
    }

    /// Starts a ledger job; parent and comment jobs also mark their scope's failures retried.
    async fn start_job(
        &self,
        family: EvidenceFamily,
        unit: ScopeUnit,
        started_at: UtcTimestamp,
        summary: &WorkSummary,
    ) -> Result<Job, EngineError> {
        let start = SyncJobStart {
            run_id: self.run.id,
            repository: &self.repository.id,
            family,
            scope_key: unit.key,
            started_at,
        };
        let id = self
            .run
            .archive
            .start_sync_job(self.run.lease, start)
            .await?;
        if matches!(family, EvidenceFamily::Threads | EvidenceFamily::Comments) {
            self.run
                .archive
                .mark_scope_failures_retried(self.run.lease, &self.scope(family, unit))
                .await?;
        }
        self.run
            .publish(summary, &self.target, SyncProgressStatus::InProgress);
        Ok(Job {
            id,
            family,
            progress: JobProgress::default(),
        })
    }

    /// Writes a job's terminal status, folds it into run accounting, and publishes progress.
    async fn finish_job(
        &self,
        job: Job,
        summary: &mut WorkSummary,
    ) -> Result<SyncJobStatus, EngineError> {
        let status = job.progress.status();
        // Comment collections already record each discussion's failure in the ledger.
        let ledger_failure = job
            .progress
            .failure()
            .filter(|_| job.family != EvidenceFamily::Comments);
        let completion = SyncJobCompletion {
            status,
            updated_at: now_utc()?,
            pages_completed: job.progress.pages_completed,
            items_committed: job.progress.items_committed,
            failure: ledger_failure,
        };
        self.run
            .archive
            .finish_sync_job(self.run.lease, job.id, completion)
            .await?;
        summary.finish_job(job.family, &job.progress);
        self.run
            .publish(summary, &self.target, progress_status(status));
        Ok(status)
    }

    /// Repository-level ledger scope used for retry marking and complete-job resolution.
    fn scope(&self, family: EvidenceFamily, unit: ScopeUnit) -> RunFailureScope<'_> {
        RunFailureScope {
            run_id: self.run.id,
            target: &self.target,
            family,
            scope_key: unit.key,
        }
    }

    /// Resolves repository-level failures once the matching job completed.
    async fn resolve_scope_failures(
        &self,
        family: EvidenceFamily,
        unit: ScopeUnit,
    ) -> Result<(), EngineError> {
        self.run
            .archive
            .resolve_scope_failures(self.run.lease, &self.scope(family, unit), now_utc()?)
            .await?;
        Ok(())
    }
}

/// Marks every selected pull-request job affected by an interrupted phase.
fn interrupt(jobs: [Option<&mut Job>; 3]) {
    for job in jobs.into_iter().flatten() {
        job.progress.interrupted = true;
    }
}

/// Reserves acquisition order and its local start time together, before provider I/O.
async fn reserve(run: &Run<'_>) -> Result<(UtcTimestamp, ObservationSequence), EngineError> {
    let started_at = now_utc()?;
    let sequence = run
        .archive
        .reserve_observation_sequence_fenced(started_at, run.lease)
        .await?;
    Ok((started_at, sequence))
}

/// Attributes an unavailable repository to each selected parent and comment job.
async fn lookup_failed(
    run: &Run<'_>,
    selector: &RepositorySelector,
    units: &[ScopeUnit],
    error: &GitHubError,
    summary: &mut WorkSummary,
) -> Result<(), EngineError> {
    let failure = github_failure(error);
    let target = selector.as_url();
    let mut families = vec![EvidenceFamily::Threads];
    if run.request.include_comments {
        families.push(EvidenceFamily::Comments);
    }
    for unit in units {
        for &family in &families {
            run.record_failure(RunFailureInput {
                run_id: run.id,
                target: &target,
                repository: None,
                thread: None,
                family: Some(family),
                scope_key: unit.key,
                failure: &failure,
                created_at: now_utc()?,
            })
            .await?;
            let mut job = JobProgress::default();
            job.add_failure(failure.clone());
            summary.finish_job(family, &job);
            run.publish(summary, &target, progress_status(job.status()));
        }
    }
    Ok(())
}

/// Moves a closed-thread sweep watermark back by a bounded overlap.
fn overlap_start(watermark: UtcTimestamp) -> UtcTimestamp {
    UtcTimestamp::from_unix_microseconds(
        watermark
            .unix_microseconds()
            .saturating_sub(CLOSED_SWEEP_OVERLAP_MICROSECONDS),
    )
    .unwrap_or(watermark)
}
