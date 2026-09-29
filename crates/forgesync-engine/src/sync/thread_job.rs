//! # Durable parent-thread scan jobs
//!
//! `ThreadJob` binds a started ledger ID to its repository scope and `ThreadScanContext`.
//! Its run method acquires pages, persists terminal status, then applies the consequences of
//! complete coverage. The closed-sweep watermark advances only after the matching scan is complete.
//!
//! Failure resolution and progress publication occur after durable updates. Partial pages and
//! thread observations remain committed when a scan fails or is cancelled; the terminal report
//! preserves their counts rather than treating them as an empty repository. Child families are
//! scheduled by the repository coordinator after this independent parent job.

use forgesync_core::coverage::{EvidenceFamily, Failure};
use forgesync_store::enumeration::RepositoryThreadScanStatus;
use forgesync_store::error::StoreError;
use forgesync_store::runs::{RunFailureScope, SyncJobCompletion};

use super::repository_work::RepositoryWork;
use super::support::{count_failure, job_result, send_progress};
use super::{SyncProgressStatus, WorkSummary};
use crate::enumeration::{
    ThreadEnumerationReport, ThreadScanContext, enumerate_repository_thread_pages, now_utc,
};
use crate::error::EngineError;
use crate::reference::RepositorySelector;

/// One durable parent-thread job whose scan and failure scope remain bound throughout execution.
pub struct ThreadJob<'a> {
    work: RepositoryWork<'a>,
    scan: ThreadScanContext,
    id: i64,
    target: String,
}

impl<'a> ThreadJob<'a> {
    /// Starts the ledger entry and marks the matching failure scope as retried.
    pub async fn start(
        work: RepositoryWork<'a>,
        scan: ThreadScanContext,
        summary: &WorkSummary,
    ) -> Result<Self, EngineError> {
        let id = work
            .archive
            .start_sync_job(
                work.context.lease,
                work.context.run_id,
                &work.repository.id,
                EvidenceFamily::Threads,
                work.unit.key,
                scan.started_at,
            )
            .await?;
        let target = RepositorySelector::from_repository(work.repository).as_url();
        let job = Self {
            work,
            scan,
            id,
            target,
        };
        work.archive
            .mark_scope_failures_retried(work.context.lease, &job.scope())
            .await?;
        job.publish(summary, SyncProgressStatus::InProgress);
        Ok(job)
    }

    /// Acquires provider pages and commits this job's final status and coverage consequences.
    pub async fn run(self, summary: &mut WorkSummary) -> Result<(), EngineError> {
        let report = enumerate_repository_thread_pages(
            self.work.archive,
            self.work.client,
            self.scan.clone(),
            Some(self.work.context.lease),
            self.work.context.cancellation,
        )
        .await?;
        self.add_counts(&report, summary)?;
        let (status, failure, progress) = job_result(&report);
        let completion = SyncJobCompletion {
            status,
            updated_at: now_utc()?,
            pages_completed: report.scan.pages_completed,
            items_committed: report.scan.threads_seen,
            failure: failure.as_ref(),
        };
        self.work
            .archive
            .finish_sync_job(self.work.context.lease, self.id, completion)
            .await?;
        self.finish_coverage(&report).await?;
        self.record_outcome(&report, failure.as_ref(), summary);
        self.publish(summary, progress);
        Ok(())
    }

    /// Adds the scan's successfully persisted pages and threads to the run's checked counts.
    fn add_counts(
        &self,
        report: &ThreadEnumerationReport,
        summary: &mut WorkSummary,
    ) -> Result<(), StoreError> {
        summary.pages_completed = summary
            .pages_completed
            .checked_add(report.scan.pages_completed)
            .ok_or(StoreError::IntegerOutOfRange)?;
        summary.threads_seen = summary
            .threads_seen
            .checked_add(report.scan.threads_seen)
            .ok_or(StoreError::IntegerOutOfRange)?;
        Ok(())
    }

    /// Advances a closed watermark and resolves scope failures only after a complete scan.
    async fn finish_coverage(&self, report: &ThreadEnumerationReport) -> Result<(), EngineError> {
        if report.scan.status != RepositoryThreadScanStatus::Complete {
            return Ok(());
        }
        if self.work.unit.update_closed_watermark {
            self.work
                .archive
                .commit_closed_sweep_watermark(
                    self.work.context.lease,
                    &self.work.repository.id,
                    self.scan.sequence,
                    self.scan.started_at,
                    now_utc()?,
                )
                .await?;
        }
        self.work
            .archive
            .resolve_scope_failures(self.work.context.lease, &self.scope(), now_utc()?)
            .await?;
        Ok(())
    }

    /// Records the terminal scan outcome without altering committed source observations.
    fn record_outcome(
        &self,
        report: &ThreadEnumerationReport,
        failure: Option<&Failure>,
        summary: &mut WorkSummary,
    ) {
        summary.completed_jobs += 1;
        if let Some(failure) = failure {
            count_failure(summary, failure);
        }
        if report.interrupted {
            summary.interrupted = true;
            summary.interrupted_jobs += 1;
        }
    }

    /// Identifies the same retry scope before acquisition and after complete coverage.
    fn scope(&self) -> RunFailureScope<'_> {
        RunFailureScope {
            run_id: self.work.context.run_id,
            target: &self.target,
            family: EvidenceFamily::Threads,
            scope_key: self.work.unit.key,
        }
    }

    /// Publishes this scope after its ledger state has been persisted.
    fn publish(&self, summary: &WorkSummary, status: SyncProgressStatus) {
        send_progress(
            &self.work.context.progress,
            self.work.context.run_id,
            summary,
            summary.total_jobs,
            Some(self.target.clone()),
            status,
        );
    }
}
