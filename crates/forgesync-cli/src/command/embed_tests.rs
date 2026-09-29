//! # Embedding diagnostic exit policy
//!
//! These cases cover stages that stop before producing an embedding report. They use the safe
//! stage diagnostic directly, so no provider, archive, credential, or process-output fixture is
//! needed to see the exit-policy expectation.
//!
//! Cancellation uses its stable process code and must retain shell interruption status. Other
//! missing-report failures are fatal even when their message happens to mention cancellation.
//! Report-bearing partial/deferred outcomes are a separate policy on `EmbeddingOutput`; they must
//! not be conflated with this pre-report boundary.

use std::process::ExitCode;

use forgesync_engine::refresh::RefreshStageFailure;

use crate::command::embed::failure_exit_status;

#[test]
fn cancellation_code_preserves_shell_interruption_status() {
    let failure = RefreshStageFailure {
        code: "operation_cancelled",
        message: "embedding acquisition stopped".to_owned(),
    };

    assert_eq!(failure_exit_status(&failure), ExitCode::from(130));
}

#[test]
fn message_text_does_not_select_cancellation_policy() {
    let failure = RefreshStageFailure {
        code: "embedding_stage_failed",
        message: "remote service mentioned cancellation".to_owned(),
    };

    assert_eq!(failure_exit_status(&failure), ExitCode::FAILURE);
}
