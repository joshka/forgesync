//! Refresh status behavior.

use super::{
    EngineError, OperationOutcome, RefreshReport, RefreshStageFailure, RefreshStageKind,
    RefreshStageStatus,
};

pub fn stage_failure(error: &EngineError) -> RefreshStageFailure {
    RefreshStageFailure {
        code: error.code(),
        message: error.to_string(),
    }
}

/// Retains the first stage failure as the report's primary diagnostic while later stages may
/// continue and record their own outcomes.
pub fn keep_first_failure(first: &mut Option<RefreshStageFailure>, candidate: RefreshStageFailure) {
    if first.is_none() {
        *first = Some(candidate);
    }
}

pub fn status_for_failure(failure: &RefreshStageFailure) -> RefreshStageStatus {
    if failure.code == "operation_cancelled" {
        RefreshStageStatus::Interrupted
    } else {
        RefreshStageStatus::Failed
    }
}

/// Lists selected stages that did not complete; an unselected stage is absent rather than
/// pending work for a later retry.
pub fn remaining_stages(report: &RefreshReport) -> Vec<RefreshStageKind> {
    let mut remaining = Vec::new();
    if report
        .sync
        .as_ref()
        .is_some_and(|stage| stage.status != RefreshStageStatus::Complete)
    {
        remaining.push(RefreshStageKind::Sync);
    }
    if report
        .embeddings
        .as_ref()
        .is_some_and(|stage| stage.status != RefreshStageStatus::Complete)
    {
        remaining.push(RefreshStageKind::Embeddings);
    }
    if report
        .clusters
        .as_ref()
        .is_some_and(|stage| stage.status != RefreshStageStatus::Complete)
    {
        remaining.push(RefreshStageKind::Clusters);
    }
    remaining
}

/// Reduces stage results to the process outcome, giving interruption precedence over other
/// partial results when unfinished work was cancelled.
pub fn refresh_outcome(report: &RefreshReport) -> OperationOutcome {
    if report.remaining.is_empty() {
        return OperationOutcome::Complete;
    }
    if report
        .remaining
        .iter()
        .any(|stage| stage_status(report, *stage) == Some(RefreshStageStatus::Interrupted))
    {
        return OperationOutcome::Interrupted {
            pending_items: u64::try_from(report.remaining.len()).unwrap_or(u64::MAX),
        };
    }
    let failed_items = u64::try_from(report.remaining.len()).unwrap_or(u64::MAX);
    let deferred_items = u64::try_from(
        report
            .remaining
            .iter()
            .filter(|stage| stage_status(report, **stage) == Some(RefreshStageStatus::Deferred))
            .count(),
    )
    .unwrap_or(u64::MAX);
    OperationOutcome::Partial {
        failed_items,
        deferred_items,
    }
}

pub fn stage_status(report: &RefreshReport, stage: RefreshStageKind) -> Option<RefreshStageStatus> {
    match stage {
        RefreshStageKind::Sync => report.sync.as_ref().map(|stage| stage.status),
        RefreshStageKind::Embeddings => report.embeddings.as_ref().map(|stage| stage.status),
        RefreshStageKind::Clusters => report.clusters.as_ref().map(|stage| stage.status),
    }
}
