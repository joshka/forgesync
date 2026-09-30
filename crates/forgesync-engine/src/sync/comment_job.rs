//! # One repository-scope comment job
//!
//! `RepositoryWork::sync_comments` starts a durable job and visits locally selected threads.
//! `CommentJob` owns accumulated pages, received comments, committed membership, failure priority,
//! and cancellation until the terminal write. Each thread's provider failure is already recorded
//! by the comment collector, so the job summary deliberately avoids a duplicate failure entry.
//!
//! The thread list is paged independently from provider comment pages. A cancellation can stop
//! either traversal; completed thread observations remain committed. Scope failures are cleared
//! only when the entire selected job is complete, while individual family failures are resolved
//! by successful collection of their own thread.

use std::num::NonZeroU32;

use forgesync_core::coverage::EvidenceFamily;
use forgesync_store::error::StoreError;
use forgesync_store::reads::{ThreadQuery, ThreadSort};
use forgesync_store::runs::{RunFailureScope, SyncJobCompletion, SyncJobStart, SyncJobStatus};

use super::SyncProgressStatus;
use super::accounting::WorkSummary;
use super::family_job::FamilyJob;
use super::repository_work::RepositoryWork;
use super::support::{send_progress, store_state_filter};
use crate::clock::now_utc;
use crate::error::EngineError;
use crate::reference::RepositorySelector;

impl RepositoryWork<'_> {
    /// Acquires comments for the archived threads selected by this repository scope.
    pub async fn sync_comments(self, summary: &mut WorkSummary) -> Result<(), EngineError> {
        let mut job = CommentJob::start(self, summary).await?;
        job.acquire().await?;
        job.finish(summary).await
    }
}

/// Started comment work whose terminal status incorporates all visited threads.
struct CommentJob<'a> {
    /// Repository selection and immutable services shared by each visited thread.
    work: RepositoryWork<'a>,
    /// Durable job identity and accumulated terminal status, committed counts, and failure
    /// priority.
    job: FamilyJob,
    /// Comments received across acquisitions, kept separate from committed membership counts.
    received: u64,
    /// Repository URL prepared once for progress messages; the provider ID remains in `work`.
    repository: String,
}

impl<'a> CommentJob<'a> {
    /// Creates the ledger entry and marks earlier scope failures as retried before traversal.
    async fn start(work: RepositoryWork<'a>, summary: &WorkSummary) -> Result<Self, EngineError> {
        let start = SyncJobStart {
            run_id: work.context.run_id,
            repository: &work.repository.id,
            family: EvidenceFamily::Comments,
            scope_key: work.unit.key,
            started_at: now_utc()?,
        };
        let id = work
            .archive
            .start_sync_job(work.context.lease, start)
            .await?;
        let repository = RepositorySelector::from_repository(work.repository).as_url();
        let job = Self {
            work,
            job: FamilyJob::new(id),
            received: 0,
            repository,
        };
        work.archive
            .mark_scope_failures_retried(work.context.lease, &job.scope())
            .await?;
        send_progress(
            &work.context.progress,
            work.context.run_id,
            summary,
            summary.total_jobs,
            Some(job.repository.clone()),
            SyncProgressStatus::InProgress,
        );
        Ok(job)
    }

    /// Pages through local thread selection, stopping once acquisition is interrupted.
    async fn acquire(&mut self) -> Result<(), EngineError> {
        let mut offset = 0;
        loop {
            if self.work.context.cancellation.is_cancelled() {
                self.job.progress.interrupted = true;
                return Ok(());
            }
            let page = self.thread_page(offset).await?;
            for thread in page.items {
                if self.acquire_thread(&thread).await? {
                    return Ok(());
                }
            }
            let Some(next) = page.next_offset else {
                return Ok(());
            };
            offset = next;
        }
    }

    /// Reads a bounded page of threads without changing archive state.
    async fn thread_page(
        &self,
        offset: u64,
    ) -> Result<forgesync_store::reads::ThreadPage, EngineError> {
        let limit = NonZeroU32::new(1000).ok_or(EngineError::InvalidPageLimit)?;
        let query = ThreadQuery {
            repositories: vec![self.work.repository.id.clone()],
            kind: None,
            state: store_state_filter(self.work.unit.state),
            match_expression: None,
            updated_since: None,
            sort: ThreadSort::Updated,
            limit,
            offset,
        };
        self.work
            .archive
            .query_threads(&query)
            .await
            .map_err(Into::into)
    }

    /// Collects one thread and folds its independent outcome into job accounting.
    async fn acquire_thread(
        &mut self,
        thread: &forgesync_store::reads::ThreadSummary,
    ) -> Result<bool, EngineError> {
        if self.work.context.cancellation.is_cancelled() {
            self.job.progress.interrupted = true;
            return Ok(true);
        }
        let work = RepositoryWork {
            repository: &thread.repository,
            ..self.work
        };
        let result = work.collect_comments(&thread.discussion).await?;
        self.job.progress.accumulate(&result)?;
        self.received = self
            .received
            .checked_add(result.items_received)
            .ok_or(StoreError::IntegerOutOfRange)?;
        Ok(result.interrupted)
    }

    /// Adds checked counts, persists terminal status, then publishes the completed-job outcome.
    ///
    /// A store error aborts the run before completion is published; accumulated source observations
    /// remain durable. Provider failures were recorded per thread, so this ledger completion does
    /// not create another failure entry for the same acquisition.
    async fn finish(self, summary: &mut WorkSummary) -> Result<(), EngineError> {
        self.add_counts(summary)?;
        let (status, failure, progress) = self.job.progress.outcome();
        let completion = SyncJobCompletion {
            status,
            updated_at: now_utc()?,
            pages_completed: self.job.progress.pages_completed,
            items_committed: self.job.progress.items_committed,
            failure: None,
        };
        self.work
            .archive
            .finish_sync_job(self.work.context.lease, self.job.id, completion)
            .await?;
        if status == SyncJobStatus::Complete {
            self.work
                .archive
                .resolve_scope_failures(self.work.context.lease, &self.scope(), now_utc()?)
                .await?;
        }
        summary.completed_jobs += 1;
        if let Some(failure) = failure {
            summary.record_failure(failure);
        }
        if self.job.progress.interrupted {
            summary.interrupted = true;
            summary.interrupted_jobs += 1;
        }
        send_progress(
            &self.work.context.progress,
            self.work.context.run_id,
            summary,
            summary.total_jobs,
            Some(self.repository.clone()),
            progress,
        );
        Ok(())
    }

    /// Adds only checked successful counts; ledger status is selected separately.
    fn add_counts(&self, summary: &mut WorkSummary) -> Result<(), StoreError> {
        summary.pages_completed = summary
            .pages_completed
            .checked_add(self.job.progress.pages_completed)
            .ok_or(StoreError::IntegerOutOfRange)?;
        summary.comments_seen = summary
            .comments_seen
            .checked_add(self.received)
            .ok_or(StoreError::IntegerOutOfRange)?;
        Ok(())
    }

    /// Identifies the same scope for retry marking and complete-job resolution.
    fn scope(&self) -> RunFailureScope<'_> {
        RunFailureScope {
            run_id: self.work.context.run_id,
            target: &self.repository,
            family: EvidenceFamily::Comments,
            scope_key: self.work.unit.key,
        }
    }
}
