//! Exit status of report-bearing embedding stages.

use forgesync_core::document::DocumentRecipe;
use forgesync_engine::embeddings::{EmbeddingBatchFailure, EmbeddingReport};
use forgesync_engine::refresh::{RefreshDocumentFailure, RefreshStageFailure, RefreshStageStatus};
use rstest::{fixture, rstest};

use crate::error::Exit;
use crate::reports::embedding::EmbeddingOutput;

#[rstest]
#[case::complete(RefreshStageStatus::Complete, Exit::Success)]
#[case::partial(RefreshStageStatus::Partial, Exit::Partial)]
#[case::deferred(RefreshStageStatus::Deferred, Exit::Partial)]
#[case::interrupted(RefreshStageStatus::Interrupted, Exit::Interrupted)]
#[case::failed(RefreshStageStatus::Failed, Exit::Failure)]
fn report_state_selects_exit_policy(
    #[case] status: RefreshStageStatus,
    #[case] expected: Exit,
    mut output: EmbeddingOutput,
) {
    output.status = status;

    assert_eq!(output.exit_status(), expected);
}

#[rstest]
fn successful_summary_has_no_failure_suffix(output: EmbeddingOutput) {
    assert_eq!(output.representative_failure(), None);
    assert_eq!(
        output.summary(),
        "Embedding complete: 0 documents, 0 chunks embedded, 0 already current, 0 failed batches, 0 document failures using local-model (https://example.com/v1)"
    );
}

#[rstest]
fn stage_failure_precedes_batch_failure(mut output: EmbeddingOutput) {
    let failure = RefreshStageFailure {
        code: "stage_failed",
        message: "stage unavailable".to_owned(),
    };
    output.failure = Some(failure);
    let batch_failure = EmbeddingBatchFailure {
        chunks: 1,
        code: "batch_failed",
        message: "batch unavailable".to_owned(),
    };
    output.report.failed_batches.push(batch_failure);
    assert_eq!(output.representative_failure(), Some("stage unavailable"));
    assert!(output.summary().ends_with("; stage unavailable"));
}

#[rstest]
fn batch_failure_precedes_document_failure(mut output: EmbeddingOutput) {
    let batch_failure = EmbeddingBatchFailure {
        chunks: 1,
        code: "batch_failed",
        message: "batch unavailable".to_owned(),
    };
    output.report.failed_batches.push(batch_failure);
    let document_failure = RefreshDocumentFailure {
        repository: "owner/repo".to_owned(),
        number: 17,
        code: "document_failed",
        message: "document unavailable".to_owned(),
    };
    output.document_failures.push(document_failure);
    assert_eq!(output.representative_failure(), Some("batch unavailable"));
    assert!(output.summary().ends_with("; batch unavailable"));
}

#[rstest]
fn document_failure_is_selected_without_stage_or_batch_failure(mut output: EmbeddingOutput) {
    let document_failure = RefreshDocumentFailure {
        repository: "owner/repo".to_owned(),
        number: 17,
        code: "document_failed",
        message: "document unavailable".to_owned(),
    };
    output.document_failures.push(document_failure);
    assert_eq!(
        output.representative_failure(),
        Some("document unavailable")
    );
    assert!(output.summary().ends_with("; document unavailable"));
}

#[fixture]
fn output() -> EmbeddingOutput {
    EmbeddingOutput {
        repositories: vec!["https://github.com/owner/repo".to_owned()],
        recipe: DocumentRecipe::DiscussionEnriched,
        endpoint: "https://example.com/v1".to_owned(),
        model: "local-model".to_owned(),
        dimensions: None,
        status: RefreshStageStatus::Complete,
        report: EmbeddingReport::default(),
        documents_materialized: 0,
        document_failures: Vec::new(),
        failure: None,
    }
}
