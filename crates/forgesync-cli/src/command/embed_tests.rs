//! # Embedding preparation, projection, and diagnostic contracts
//!
//! These cases exercise canonical repository scope and prepared output without provider requests
//! or archive fixtures. Service clients use explicit static test keys through `client_config`,
//! avoiding process credential discovery. The selected identity and partial report facts remain
//! visible in each test rather than being supplied by a scenario helper.
//!
//! Missing-report diagnostics retain a concrete safe fallback. Cancellation classification uses
//! its stable process code, not message wording. Report-bearing partial/deferred exit policy lives
//! beside `EmbeddingOutput` and has its own named cases in the reports module. Together these tests
//! distinguish absent acquisition results from useful partial results without parsing output text.

use std::process::ExitCode;

use forgesync_core::document::DocumentRecipe;
use forgesync_engine::embedding_client::EmbeddingClient;
use forgesync_engine::embeddings::{EmbeddingBatchFailure, EmbeddingPolicy, EmbeddingReport};
use forgesync_engine::refresh::{
    RefreshDocumentFailure, RefreshEmbeddingReport, RefreshStage, RefreshStageFailure,
    RefreshStageStatus,
};

use crate::command::embed::{PreparedEmbedding, failure_exit_status, repository_scope};
use crate::config::EmbeddingServiceConfig;

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

#[test]
fn repository_scope_is_unique_and_ordered_by_canonical_url() {
    let repositories = vec![
        "owner/z".parse().expect("repository z"),
        "owner/a".parse().expect("repository a"),
        "owner/z".parse().expect("duplicate repository z"),
    ];

    let scope = repository_scope(repositories);

    assert_eq!(scope.len(), 2);
    assert_eq!(scope[0].as_url(), "https://github.com/owner/a");
    assert_eq!(scope[1].as_url(), "https://github.com/owner/z");
}

#[test]
fn absent_stage_report_has_a_concrete_fallback_diagnostic() {
    let service = EmbeddingServiceConfig::default();
    let client_config = service
        .client_config("static-test-key".to_owned())
        .expect("service config");
    let client = EmbeddingClient::new(client_config).expect("client");
    let prepared = PreparedEmbedding {
        repositories: vec!["owner/repo".parse().expect("repository")],
        client,
        recipe: DocumentRecipe::DiscussionEnriched,
        policy: EmbeddingPolicy::Missing,
        dimensions: None,
    };
    let stage = RefreshStage {
        status: RefreshStageStatus::Failed,
        report: None,
        failure: None,
    };

    let failure = prepared
        .output(stage)
        .expect_err("missing report diagnostic");

    assert_eq!(failure.code, "embedding_stage_failed");
    assert_eq!(failure.message, "embedding stage did not produce a report");
}

#[test]
fn partial_output_keeps_execution_identity_counts_and_safe_failure() {
    let service = EmbeddingServiceConfig {
        endpoint: "https://example.com/v1/".to_owned(),
        model: "local-model".to_owned(),
        dimensions: Some(2),
        ..Default::default()
    };
    let client_config = service
        .client_config("static-test-key".to_owned())
        .expect("service config");
    let client = EmbeddingClient::new(client_config).expect("client");
    let prepared = PreparedEmbedding {
        repositories: vec!["owner/repo".parse().expect("repository")],
        client,
        recipe: DocumentRecipe::DiscussionEnriched,
        policy: EmbeddingPolicy::Replace,
        dimensions: Some(2),
    };
    let stage = RefreshStage {
        status: RefreshStageStatus::Partial,
        report: Some(RefreshEmbeddingReport {
            documents_materialized: 2,
            embeddings: EmbeddingReport {
                documents: 2,
                chunks_selected: 4,
                chunks_embedded: 3,
                failed_batches: vec![EmbeddingBatchFailure {
                    chunks: 1,
                    code: "embedding_service_failed",
                    message: "embedding service returned HTTP 503".to_owned(),
                }],
                ..Default::default()
            },
            document_failures: vec![RefreshDocumentFailure {
                repository: "https://github.com/owner/repo".to_owned(),
                number: 3,
                code: "thread_missing",
                message: "discussion unavailable".to_owned(),
            }],
        }),
        failure: Some(RefreshStageFailure {
            code: "provider_response",
            message: "safe failure".to_owned(),
        }),
    };

    let output = prepared.output(stage).expect("partial report");

    assert_eq!(output.repositories, ["https://github.com/owner/repo"]);
    assert_eq!(output.endpoint, "https://example.com/v1");
    assert_eq!(output.model, "local-model");
    assert_eq!(output.dimensions, Some(2));
    assert_eq!(output.recipe, DocumentRecipe::DiscussionEnriched);
    assert_eq!(output.status, RefreshStageStatus::Partial);
    assert_eq!(output.documents_materialized, 2);
    assert_eq!(output.report.documents, 2);
    assert_eq!(output.report.chunks_selected, 4);
    assert_eq!(output.report.chunks_embedded, 3);
    assert_eq!(output.report.failed_batches.len(), 1);
    assert_eq!(output.report.failed_batches[0].chunks, 1);
    assert_eq!(
        output.report.failed_batches[0].code,
        "embedding_service_failed"
    );
    assert_eq!(output.document_failures.len(), 1);
    assert_eq!(output.document_failures[0].number, 3);
    assert_eq!(output.document_failures[0].code, "thread_missing");
    let failure = output.failure.expect("safe partial failure");
    assert_eq!(failure.code, "provider_response");
    assert_eq!(failure.message, "safe failure");
}
