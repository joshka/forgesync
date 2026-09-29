//! Refresh status behavior.

use super::*;

pub(super) fn stage_failure(error: &EngineError) -> RefreshStageFailure {
    RefreshStageFailure {
        code: error.code(),
        message: error.to_string(),
    }
}

pub(super) fn keep_first_failure(
    first: &mut Option<RefreshStageFailure>,
    candidate: RefreshStageFailure,
) {
    if first.is_none() {
        *first = Some(candidate);
    }
}

pub(super) fn status_for_failure(failure: &RefreshStageFailure) -> RefreshStageStatus {
    if failure.code == "operation_cancelled" {
        RefreshStageStatus::Interrupted
    } else {
        RefreshStageStatus::Failed
    }
}

pub(super) fn remaining_stages(report: &RefreshReport) -> Vec<RefreshStageKind> {
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

pub(super) fn refresh_outcome(report: &RefreshReport) -> OperationOutcome {
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

pub(super) fn stage_status(
    report: &RefreshReport,
    stage: RefreshStageKind,
) -> Option<RefreshStageStatus> {
    match stage {
        RefreshStageKind::Sync => report.sync.as_ref().map(|stage| stage.status),
        RefreshStageKind::Embeddings => report.embeddings.as_ref().map(|stage| stage.status),
        RefreshStageKind::Clusters => report.clusters.as_ref().map(|stage| stage.status),
    }
}
