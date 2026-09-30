//! # Acquire selected pull-request evidence as independent durable jobs
//!
//! `RepositoryWork::sync_pull_requests` discovers archived pull requests in the selected scope,
//! then starts metadata and the requested review-family jobs. `PullRequestJobs` owns those started
//! jobs throughout traversal, with optional jobs representing families the user actually selected.
//!
//! Each target acquires metadata first, then reviews and review threads against that result. A
//! failed family is accumulated without discarding sibling successes. Cancellation marks exactly
//! the jobs affected by the interrupted phase. Terminal writes consume the started jobs, publish
//! progress, and preserve their page/member counts in the durable ledger.

use std::num::NonZeroU32;

use forgesync_core::content::{PullRequestMetadata, ThreadKind};
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::error::StoreError;
use forgesync_store::reads::{ThreadQuery, ThreadSort};
use forgesync_store::runs::SyncJobStart;

use super::SyncProgressStatus;
use super::accounting::WorkSummary;
use super::family_job::FamilyJob;
use super::repository_work::RepositoryWork;
use super::review_collection::{ReviewFamily, ReviewSync};
use super::support::store_state_filter;
use crate::clock::now_utc;
use crate::error::EngineError;
use crate::reference::RepositorySelector;
use crate::sync::scope::{PullRequestTarget, ThreadFamilyResult, ThreadFamilyScope};

impl<'a> RepositoryWork<'a> {
    /// Runs the selected pull-request families with metadata preceding head-bound review evidence.
    pub async fn sync_pull_requests(self, summary: &mut WorkSummary) -> Result<(), EngineError> {
        let targets = self.pull_request_targets().await?;
        if targets.is_empty() {
            return Ok(());
        }
        let mut jobs = self.start_pull_request_jobs(summary).await?;
        jobs.acquire(targets, summary).await?;
        jobs.finish(summary).await
    }

    /// Starts exactly the requested family jobs before traversal and announces their shared scope.
    async fn start_pull_request_jobs(
        self,
        summary: &mut WorkSummary,
    ) -> Result<PullRequestJobs<'a>, EngineError> {
        let added = 1
            + u64::from(self.context.include_reviews)
            + u64::from(self.context.include_review_threads);
        summary.total_jobs = summary
            .total_jobs
            .checked_add(added)
            .ok_or(StoreError::IntegerOutOfRange)?;
        let started_at = now_utc()?;
        let metadata = self
            .start_pull_request_job(EvidenceFamily::PullRequestMetadata, started_at)
            .await?;
        let reviews = if self.context.include_reviews {
            Some(
                self.start_pull_request_job(EvidenceFamily::Reviews, started_at)
                    .await?,
            )
        } else {
            None
        };
        let review_threads = if self.context.include_review_threads {
            Some(
                self.start_pull_request_job(EvidenceFamily::ReviewThreads, started_at)
                    .await?,
            )
        } else {
            None
        };
        let repository = RepositorySelector::from_repository(self.repository).as_url();
        self.context
            .publish(summary, Some(repository), SyncProgressStatus::InProgress);
        Ok(PullRequestJobs {
            work: self,
            metadata,
            reviews,
            review_threads,
        })
    }

    /// Creates the durable ID that must later be paired with a consumed family-job result.
    async fn start_pull_request_job(
        &self,
        family: EvidenceFamily,
        started_at: UtcTimestamp,
    ) -> Result<FamilyJob, EngineError> {
        let start = SyncJobStart {
            run_id: self.context.run_id,
            repository: &self.repository.id,
            family,
            scope_key: self.unit.key,
            started_at,
        };
        let id = self
            .archive
            .start_sync_job(self.context.lease, start)
            .await?;
        Ok(FamilyJob::new(id))
    }

    /// Selects archived pull requests in the scope without fetching provider evidence yet.
    async fn pull_request_targets(&self) -> Result<Vec<PullRequestTarget>, EngineError> {
        let page_limit = NonZeroU32::new(1000).ok_or(EngineError::InvalidPageLimit)?;
        let mut offset = 0_u64;
        let mut targets = Vec::new();
        loop {
            let page = self
                .archive
                .query_threads(&ThreadQuery {
                    repositories: vec![self.repository.id.clone()],
                    kind: Some(ThreadKind::PullRequest),
                    state: store_state_filter(self.unit.state),
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
}

/// Started family jobs belonging to one immutable repository execution scope.
struct PullRequestJobs<'a> {
    /// Repository scope and shared services used by all selected family jobs.
    work: RepositoryWork<'a>,
    /// Mandatory metadata job, whose head result supplies context for both review traversals.
    metadata: FamilyJob,
    /// Started review job only when requested; absence means unselected rather than failed.
    reviews: Option<FamilyJob>,
    /// Independently started review-thread job only when its traversal was requested.
    review_threads: Option<FamilyJob>,
}

impl PullRequestJobs<'_> {
    /// Visits targets until traversal completes or a phase reports cancellation.
    async fn acquire(
        &mut self,
        targets: Vec<PullRequestTarget>,
        summary: &mut WorkSummary,
    ) -> Result<(), EngineError> {
        for target in targets {
            if self.work.context.cancellation.is_cancelled() {
                self.interrupt_all();
                break;
            }
            if self.acquire_target(&target, summary).await? {
                break;
            }
        }
        Ok(())
    }

    /// Acquires metadata and the selected review families for a single target.
    async fn acquire_target(
        &mut self,
        target: &PullRequestTarget,
        summary: &mut WorkSummary,
    ) -> Result<bool, EngineError> {
        let scope = ThreadFamilyScope {
            repository: self.work.repository,
            thread: &target.thread,
            updated_at: target.updated_at,
            key: self.work.unit.key,
        };
        let metadata = scope
            .sync_metadata(self.work.archive, self.work.client, self.work.context)
            .await?;
        self.metadata.progress.accumulate(&metadata)?;
        summary.pull_request_metadata_seen = summary
            .pull_request_metadata_seen
            .checked_add(metadata.items_received)
            .ok_or(StoreError::IntegerOutOfRange)?;
        if metadata.interrupted {
            self.interrupt_all();
            return Ok(true);
        }
        if self.acquire_reviews(scope, &metadata, summary).await? {
            self.interrupt_all();
            return Ok(true);
        }
        self.acquire_review_threads(scope, &metadata, summary).await
    }

    /// Runs a requested REST review job; an absent job represents an unselected family.
    async fn acquire_reviews(
        &mut self,
        scope: ThreadFamilyScope<'_>,
        metadata: &ThreadFamilyResult<PullRequestMetadata>,
        summary: &mut WorkSummary,
    ) -> Result<bool, EngineError> {
        let Some(job) = self.reviews.as_mut() else {
            return Ok(false);
        };
        let sync = ReviewSync::new(
            self.work.archive,
            self.work.client,
            scope,
            self.work.context,
            ReviewFamily::Reviews,
        );
        let result = sync.run(metadata).await?;
        job.progress.accumulate(&result)?;
        summary.reviews_seen = summary
            .reviews_seen
            .checked_add(result.items_received)
            .ok_or(StoreError::IntegerOutOfRange)?;
        Ok(result.interrupted)
    }

    /// Runs requested GraphQL work, preserving a completed REST review job on interruption.
    async fn acquire_review_threads(
        &mut self,
        scope: ThreadFamilyScope<'_>,
        metadata: &ThreadFamilyResult<PullRequestMetadata>,
        summary: &mut WorkSummary,
    ) -> Result<bool, EngineError> {
        let Some(job) = self.review_threads.as_mut() else {
            return Ok(false);
        };
        let sync = ReviewSync::new(
            self.work.archive,
            self.work.client,
            scope,
            self.work.context,
            ReviewFamily::ReviewThreads,
        );
        let result = sync.run(metadata).await?;
        job.progress.accumulate(&result)?;
        summary.review_threads_seen = summary
            .review_threads_seen
            .checked_add(result.items_received)
            .ok_or(StoreError::IntegerOutOfRange)?;
        if result.interrupted {
            self.metadata.progress.interrupted = true;
        }
        Ok(result.interrupted)
    }

    /// Marks all selected families when cancellation prevents remaining phases from running.
    fn interrupt_all(&mut self) {
        self.metadata.progress.interrupted = true;
        if let Some(job) = &mut self.reviews {
            job.progress.interrupted = true;
        }
        if let Some(job) = &mut self.review_threads {
            job.progress.interrupted = true;
        }
    }

    /// Writes each selected terminal job after adding its page count to the run summary.
    async fn finish(self, summary: &mut WorkSummary) -> Result<(), EngineError> {
        let pages = self.metadata.progress.pages_completed;
        let review_pages = self
            .reviews
            .as_ref()
            .map_or(0, |job| job.progress.pages_completed);
        let thread_pages = self
            .review_threads
            .as_ref()
            .map_or(0, |job| job.progress.pages_completed);
        summary.pages_completed = summary
            .pages_completed
            .checked_add(pages)
            .and_then(|count| count.checked_add(review_pages))
            .and_then(|count| count.checked_add(thread_pages))
            .ok_or(StoreError::IntegerOutOfRange)?;
        let repository = RepositorySelector::from_repository(self.work.repository).as_url();
        self.metadata
            .finish(self.work.archive, self.work.context, summary, &repository)
            .await?;
        if let Some(job) = self.reviews {
            job.finish(self.work.archive, self.work.context, summary, &repository)
                .await?;
        }
        if let Some(job) = self.review_threads {
            job.finish(self.work.archive, self.work.context, summary, &repository)
                .await?;
        }
        Ok(())
    }
}
