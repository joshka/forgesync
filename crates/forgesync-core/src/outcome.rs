//! Structured outcomes for complete, partial, deferred, interrupted, and failed work.

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

    #[test]
    fn terminal_outcomes_have_distinct_machine_readable_states() {
        let cases = [
            (OperationOutcome::Complete, json!({ "status": "complete" })),
            (
                OperationOutcome::Partial {
                    failed_items: 1,
                    deferred_items: 2,
                },
                json!({ "status": "partial", "failed_items": 1, "deferred_items": 2 }),
            ),
            (
                OperationOutcome::Deferred {
                    reason: DeferredReason::Offline,
                },
                json!({ "status": "deferred", "reason": "offline" }),
            ),
            (
                OperationOutcome::Failed {
                    failure: Failure {
                        kind: FailureKind::Archive,
                        message: "archive could not be committed".to_owned(),
                    },
                },
                json!({ "status": "failed", "failure": { "kind": "archive", "message": "archive could not be committed" } }),
            ),
            (
                OperationOutcome::Interrupted { pending_items: 3 },
                json!({ "status": "interrupted", "pending_items": 3 }),
            ),
        ];

        for (outcome, expected) in cases {
            let encoded = serde_json::to_value(&outcome).expect("serialize outcome");
            assert_eq!(encoded, expected);
            let decoded: OperationOutcome =
                serde_json::from_value(encoded).expect("deserialize outcome");
            assert_eq!(decoded, outcome);
        }
    }
}
