//! Stage-failure classification and the combined refresh outcome.
//!
//! Outcome units are selected stages, not threads, documents, or clusters. Partial `failed_items`
//! counts every unfinished stage and `deferred_items` its deferred subset; those counts overlap.

use forgesync_core::outcome::OperationOutcome;

use crate::error::EngineError;
use crate::refresh::{RefreshReport, RefreshStageFailure, RefreshStageKind, RefreshStageStatus};

/// A stage diagnostic plus whether it records caller cancellation rather than a failure.
pub struct StageFailure {
    pub failure: RefreshStageFailure,
    pub cancelled: bool,
}

impl StageFailure {
    /// Copies the engine classification and safe display summary into a stage diagnostic.
    pub fn from_error(error: &EngineError) -> Self {
        Self {
            failure: RefreshStageFailure {
                code: error.code(),
                message: error.to_string(),
            },
            cancelled: error.is_cancelled(),
        }
    }

    /// A failure that is not a cancellation.
    pub fn failed(failure: RefreshStageFailure) -> Self {
        Self {
            failure,
            cancelled: false,
        }
    }

    /// Status of a stage that produced no report because of this failure.
    pub fn status(&self) -> RefreshStageStatus {
        if self.cancelled {
            RefreshStageStatus::Interrupted
        } else {
            RefreshStageStatus::Failed
        }
    }
}

/// Retains the first failure as the stage's primary diagnostic.
pub fn keep_first_failure(first: &mut Option<StageFailure>, candidate: StageFailure) {
    if first.is_none() {
        *first = Some(candidate);
    }
}

impl From<&OperationOutcome> for RefreshStageStatus {
    fn from(outcome: &OperationOutcome) -> Self {
        match outcome {
            OperationOutcome::Complete => Self::Complete,
            OperationOutcome::Partial { .. } => Self::Partial,
            OperationOutcome::Deferred { .. } => Self::Deferred,
            OperationOutcome::Failed { .. } => Self::Failed,
            OperationOutcome::Interrupted { .. } => Self::Interrupted,
        }
    }
}

impl RefreshReport {
    /// Derives `remaining` (selected stages that did not complete) and the combined outcome;
    /// any interrupted unfinished stage makes the whole refresh interrupted.
    pub(super) fn finish(&mut self) {
        let statuses = [
            (
                RefreshStageKind::Sync,
                self.sync.as_ref().map(|stage| stage.status),
            ),
            (
                RefreshStageKind::Embeddings,
                self.embeddings.as_ref().map(|stage| stage.status),
            ),
            (
                RefreshStageKind::Clusters,
                self.clusters.as_ref().map(|stage| stage.status),
            ),
        ];
        let unfinished = statuses
            .into_iter()
            .filter_map(|(kind, status)| Some((kind, status?)))
            .filter(|(_, status)| *status != RefreshStageStatus::Complete)
            .collect::<Vec<_>>();
        self.remaining = unfinished.iter().map(|(kind, _)| *kind).collect();
        let count = |wanted: RefreshStageStatus| {
            unfinished
                .iter()
                .filter(|(_, status)| *status == wanted)
                .count() as u64
        };
        self.outcome = if unfinished.is_empty() {
            OperationOutcome::Complete
        } else if count(RefreshStageStatus::Interrupted) > 0 {
            OperationOutcome::Interrupted {
                pending_items: unfinished.len() as u64,
            }
        } else {
            OperationOutcome::Partial {
                failed_items: unfinished.len() as u64,
                deferred_items: count(RefreshStageStatus::Deferred),
            }
        };
    }
}
