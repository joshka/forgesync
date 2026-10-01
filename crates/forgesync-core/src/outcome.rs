//! Structured terminal status for local and provider-backed workflows.
//!
//! A workflow can commit useful pages and still report a partial result; reducing that state to an
//! `Err` or a success boolean would lose the retry and presentation information. Counts describe
//! the owning workflow's work units.

use serde::{Deserialize, Serialize};

use crate::coverage::{DeferredReason, Failure};

/// Terminal state of a local operation after its durable work has been accounted for.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum OperationOutcome {
    /// Every selected family completed successfully.
    Complete,
    /// Some work committed, while selected work failed or remains deferred.
    Partial {
        /// Number of failed work units.
        failed_items: u64,
        /// Number of deferred work units.
        deferred_items: u64,
    },
    /// No required work began because policy deferred the operation.
    Deferred {
        /// Reason the operation was postponed.
        reason: DeferredReason,
    },
    /// The operation failed before a reportable partial result could be produced.
    Failed {
        /// Safe structured failure summary.
        failure: Failure,
    },
    /// The caller cancelled or the process stopped while retryable work remained.
    Interrupted {
        /// Number of work units still pending recovery.
        pending_items: u64,
    },
}

impl OperationOutcome {
    /// Returns the serde status tag of this outcome.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial { .. } => "partial",
            Self::Deferred { .. } => "deferred",
            Self::Failed { .. } => "failed",
            Self::Interrupted { .. } => "interrupted",
        }
    }
}

#[cfg(test)]
mod tests {
    //! Every outcome variant has a distinct JSON status and round-trips.

    use serde_json::json;

    use crate::coverage::{DeferredReason, Failure, FailureKind};
    use crate::outcome::OperationOutcome;

    #[rstest::rstest]
    #[case::complete(OperationOutcome::Complete, json!({ "status": "complete" }))]
    #[case::partial(
        OperationOutcome::Partial { failed_items: 1, deferred_items: 2 },
        json!({ "status": "partial", "failed_items": 1, "deferred_items": 2 })
    )]
    #[case::deferred(
        OperationOutcome::Deferred { reason: DeferredReason::Offline },
        json!({ "status": "deferred", "reason": "offline" })
    )]
    #[case::failed(
        OperationOutcome::Failed { failure: Failure {
            kind: FailureKind::Archive,
            message: "archive could not be committed".to_owned(),
        } },
        json!({ "status": "failed", "failure": {
            "kind": "archive", "message": "archive could not be committed"
        } })
    )]
    #[case::interrupted(
        OperationOutcome::Interrupted { pending_items: 3 },
        json!({ "status": "interrupted", "pending_items": 3 })
    )]
    fn terminal_outcomes_have_distinct_machine_readable_states(
        #[case] outcome: OperationOutcome,
        #[case] expected: serde_json::Value,
    ) {
        let encoded = serde_json::to_value(&outcome).expect("serialize outcome");
        assert_eq!(encoded, expected);
        assert_eq!(encoded["status"], outcome.as_str());
        let decoded: OperationOutcome =
            serde_json::from_value(encoded).expect("deserialize outcome");
        assert_eq!(decoded, outcome);
    }
}
