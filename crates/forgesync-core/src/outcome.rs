//! Structured terminal status for local and provider-backed workflows.
//!
//! [`OperationOutcome`] distinguishes complete, partial, deferred, interrupted, and failed work. A
//! workflow can commit useful pages and still report a partial result; reducing that state to an
//! `Err` or a success boolean would lose the retry and presentation information.
//!
//! Engine reports carry this value with counts and per-family failures. CLI and TUI map it to
//! human text, JSON, and process status without parsing diagnostic strings. Store run records
//! preserve the same distinction for later inspection and retry planning.
//!
//! Use this enum to describe the outcome of an operation as a whole. Use [`crate::coverage`] for
//! the state of each evidence family, and typed errors for a call that cannot return a report.

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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::OperationOutcome;
    use crate::coverage::{DeferredReason, Failure, FailureKind};

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
        let decoded: OperationOutcome =
            serde_json::from_value(encoded).expect("deserialize outcome");
        assert_eq!(decoded, outcome);
    }
}
