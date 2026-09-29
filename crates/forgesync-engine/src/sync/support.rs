//! # Sync attribution, state mapping, and progress presentation
//!
//! Support functions resolve user selectors, map requested state filters, turn job outcomes into
//! progress, and record thread-family failures. They centralize cross-family policy without owning
//! any provider pagination. Run-wide counters and outcome selection belong to `accounting`;
//! this module translates individual job evidence into ledger inputs and progress snapshots.
//!
//! A failure is tied to the thread and family that produced it. This lets the run ledger and
//! report retain partial success and gives retry a precise target rather than a generic failed-run
//! flag.

use forgesync_core::coverage::{EvidenceFamily, Failure, FailureKind};
use forgesync_core::identity::{RunId, ThreadId};
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_github::resources::ThreadListState;
use forgesync_store::archive::Archive;
use forgesync_store::enumeration::RepositoryThreadScanStatus;
use forgesync_store::reads::ThreadStateFilter;
use forgesync_store::runs::{RunFailureInput, SyncJobStatus};
use tokio::sync::mpsc;

use super::accounting::WorkSummary;
use super::{
    CLOSED_SWEEP_OVERLAP_MICROSECONDS, SyncProgress, SyncProgressStatus, SyncRequest,
    SyncRunContext,
};
use crate::enumeration::{ThreadEnumerationReport, now_utc};
use crate::error::EngineError;
use crate::reference::RepositorySelector;

/// Persists one family failure without discarding other acquired evidence.
pub async fn record_thread_family_failure(
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

/// Maps a provider thread-state scope to its local query filter.
pub fn store_state_filter(state: ThreadListState) -> ThreadStateFilter {
    match state {
        ThreadListState::All => ThreadStateFilter::All,
        ThreadListState::Open => ThreadStateFilter::Open,
        ThreadListState::Closed => ThreadStateFilter::Closed,
    }
}

/// Resolves explicit or all-registered repositories before provider acquisition.
pub async fn resolve_selectors(
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

/// Builds a durable job completion from one selected family outcome.
pub fn job_result(
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

/// Maps a structured failure to the progress status shown to callers.
pub fn progress_status(failure: &Failure) -> SyncProgressStatus {
    if failure.kind == FailureKind::RateLimited {
        SyncProgressStatus::Deferred
    } else {
        SyncProgressStatus::Failed
    }
}

/// Adds a bounded overlap to a closed-thread sweep watermark.
pub fn overlap_start(watermark: UtcTimestamp) -> UtcTimestamp {
    UtcTimestamp::from_unix_microseconds(
        watermark
            .unix_microseconds()
            .saturating_sub(CLOSED_SWEEP_OVERLAP_MICROSECONDS),
    )
    .unwrap_or(watermark)
}

/// Sends an opportunistic progress snapshot without blocking acquisition.
pub fn send_progress(
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
