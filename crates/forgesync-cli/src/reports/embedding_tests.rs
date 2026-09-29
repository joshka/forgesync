//! # Exit policy for report-bearing embedding stages
//!
//! Each named case supplies the authoritative stage state and expected process result. The output
//! has an empty but present engine report, which distinguishes these cases from acquisition that
//! fails before producing any report.
//!
//! Partial and deferred results preserve useful work and use the retryable result code. Interrupted
//! work retains shell cancellation semantics, while complete and failed stages have ordinary
//! success and failure results. No message parsing, provider fixture, or hidden scenario helper
//! selects the expected policy; the state-to-result mapping is visible in the case table.

use std::process::ExitCode;

use forgesync_core::document::DocumentRecipe;
use forgesync_engine::embeddings::EmbeddingReport;
use forgesync_engine::refresh::RefreshStageStatus;
use rstest::rstest;

use crate::reports::embedding::EmbeddingOutput;

#[rstest]
#[case::complete(RefreshStageStatus::Complete, ExitCode::SUCCESS)]
#[case::partial(RefreshStageStatus::Partial, ExitCode::from(3))]
#[case::deferred(RefreshStageStatus::Deferred, ExitCode::from(3))]
#[case::interrupted(RefreshStageStatus::Interrupted, ExitCode::from(130))]
#[case::failed(RefreshStageStatus::Failed, ExitCode::FAILURE)]
fn report_state_selects_exit_policy(
    #[case] status: RefreshStageStatus,
    #[case] expected: ExitCode,
) {
    let output = EmbeddingOutput {
        repositories: vec!["https://github.com/owner/repo".to_owned()],
        recipe: DocumentRecipe::DiscussionEnriched,
        endpoint: "https://example.com/v1".to_owned(),
        model: "local-model".to_owned(),
        dimensions: None,
        status,
        report: EmbeddingReport::default(),
        documents_materialized: 0,
        document_failures: Vec::new(),
        failure: None,
    };

    assert_eq!(output.exit_status(), expected);
}
