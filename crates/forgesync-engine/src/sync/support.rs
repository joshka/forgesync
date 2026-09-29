//! Support sync work.

use super::{
    Archive, CLOSED_SWEEP_OVERLAP_MICROSECONDS, DeferredReason, EngineError, EvidenceFamily,
    Failure, FailureKind, OperationOutcome, RepositorySelector, RepositoryThreadScanStatus,
    RunFailureInput, RunId, SyncJobStatus, SyncProgress, SyncProgressStatus, SyncRequest,
    SyncRunContext, ThreadEnumerationReport, ThreadId, ThreadListState, ThreadStateFilter,
    UtcTimestamp, WorkSummary, mpsc, now_utc,
};

pub(super) async fn record_thread_family_failure(
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

pub(super) fn store_state_filter(state: ThreadListState) -> ThreadStateFilter {
    match state {
        ThreadListState::All => ThreadStateFilter::All,
        ThreadListState::Open => ThreadStateFilter::Open,
        ThreadListState::Closed => ThreadStateFilter::Closed,
    }
}

pub(super) async fn resolve_selectors(
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

pub(super) fn job_result(
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

pub(super) fn progress_status(failure: &Failure) -> SyncProgressStatus {
    if failure.kind == FailureKind::RateLimited {
        SyncProgressStatus::Deferred
    } else {
        SyncProgressStatus::Failed
    }
}

pub(super) fn count_failure(summary: &mut WorkSummary, failure: &Failure) {
    if failure.kind == FailureKind::RateLimited {
        summary.deferred_jobs += 1;
    } else {
        summary.failed_jobs += 1;
    }
    if summary.first_failure.is_none() {
        summary.first_failure = Some(failure.clone());
    }
}

pub(super) fn operation_outcome(work: &WorkSummary) -> OperationOutcome {
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

pub(super) fn overlap_start(watermark: UtcTimestamp) -> UtcTimestamp {
    UtcTimestamp::from_unix_microseconds(
        watermark
            .unix_microseconds()
            .saturating_sub(CLOSED_SWEEP_OVERLAP_MICROSECONDS),
    )
    .unwrap_or(watermark)
}

pub(super) fn send_progress(
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
