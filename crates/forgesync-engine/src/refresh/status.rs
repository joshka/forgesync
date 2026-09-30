//! # Convert stage errors into stable report status
//!
//! Status helpers classify an `EngineError`, preserve the first failure, and determine which
//! stages remain. Keeping these rules in one place avoids subtly different partial-success
//! behavior in sync, embedding, and cluster branches.
//!
//! A stage status is an account of attempted work. It should not imply that unrequested stages
//! ran, or that a successful earlier stage is undone by a later failure.
//!
//! `remaining_stages` projects selected noncomplete stages in sync/embedding/cluster order. It
//! derives from optional stage records, not the report's existing remaining list. The coordinator
//! assigns that list before calling `refresh_outcome`, which trusts it as its accounting input.
//!
//! Outcome units here are selected stages, not threads, documents, requests, or clusters. An
//! empty remaining list yields complete; any interrupted remaining stage gives interruption
//! precedence. Otherwise partial failure counts every remaining stage and separately reports the
//! deferred subset. Those counts overlap; they must not be summed as disjoint work categories.
//!
//! Primary failure selection retains the earliest supplied failure while individual stages retain
//! their own reports. Cancellation classification recognizes the workflow's operation-cancelled
//! code; it does not inspect arbitrary message text. These helpers only project supplied records:
//! no provider calls, ledger mutation, retry scheduling, or completeness validation occurs here.

use forgesync_core::outcome::OperationOutcome;

use crate::error::EngineError;
use crate::refresh::{RefreshReport, RefreshStageFailure, RefreshStageKind, RefreshStageStatus};

/// Copies the engine classification and display summary into a stage diagnostic.
///
/// Relies on the typed engine error's safe display contract; this projection does not redact
/// arbitrary text or retain the source chain. It does not determine stage status.
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

/// Selects a stage status from the recorded failure category.
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
///
/// Requires `remaining` to have been refreshed from the stage records. Counts are stage units:
/// partial `failed_items` includes every remaining stage and `deferred_items` is its deferred
/// subset. These are overlapping counts rather than independent populations. This projection
/// trusts the report and does not independently reconcile an inconsistent remaining list.
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

/// Looks up the selected stage's terminal status in a refresh report.
pub fn stage_status(report: &RefreshReport, stage: RefreshStageKind) -> Option<RefreshStageStatus> {
    match stage {
        RefreshStageKind::Sync => report.sync.as_ref().map(|stage| stage.status),
        RefreshStageKind::Embeddings => report.embeddings.as_ref().map(|stage| stage.status),
        RefreshStageKind::Clusters => report.clusters.as_ref().map(|stage| stage.status),
    }
}
