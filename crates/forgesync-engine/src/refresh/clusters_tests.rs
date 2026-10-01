//! Refresh cluster outcome accounting.

use forgesync_core::document::DocumentRecipe;
use forgesync_store::clusters::ClusterGenerationResult;
use rstest::rstest;

use crate::clustering::{ClusterBuildReport, ClusterOptions};
use crate::error::EngineError;
use crate::reference::RepositorySelector;
use crate::refresh::RefreshStageStatus;
use crate::refresh::clusters::{ClusterStage, missing_identity};
use crate::refresh::status::StageFailure;

#[test]
fn empty_repository_scope_completes_without_invented_work() {
    let report = test_stage().finish();

    assert_eq!(report.status, RefreshStageStatus::Complete);
    assert!(report.report.expect("stage report").is_empty());
    assert!(report.failure.is_none());
}

#[test]
fn a_complete_generation_completes_the_stage() {
    let mut stage = test_stage();
    let repository = "owner/repo".parse().expect("repository");

    stage.record_success(&repository, complete_report());
    let report = stage.finish();

    assert_eq!(report.status, RefreshStageStatus::Complete);
    assert_eq!(report.report.expect("stage report").len(), 1);
    assert!(report.failure.is_none());
}

#[test]
fn cancellation_before_an_attempt_is_interrupted_without_a_repository_result() {
    let mut stage = test_stage();

    stage.interrupt();
    let report = stage.finish();

    assert_eq!(report.status, RefreshStageStatus::Interrupted);
    assert!(report.report.expect("attempted repositories").is_empty());
    assert_eq!(
        report.failure.expect("cancellation diagnostic").code,
        "operation_cancelled"
    );
}

#[test]
fn incomplete_vector_coverage_keeps_a_successful_generation_but_marks_the_stage_partial() {
    let mut stage = test_stage();
    let repository = "owner/repo".parse().expect("repository");
    let mut generation = complete_report();
    generation.generation.complete_coverage = false;
    generation.vector_threads = 1;

    stage.record_success(&repository, generation);
    let report = stage.finish();

    assert_eq!(report.status, RefreshStageStatus::Partial);
    assert_eq!(report.report.expect("stage report").len(), 1);
    assert!(report.failure.is_none());
}

#[rstest]
#[case::missing_identity(
    StageFailure::failed(missing_identity()),
    "embedding_service_identity_missing",
    RefreshStageStatus::Failed
)]
#[case::interrupted(
    StageFailure::from_error(&EngineError::Cancelled),
    "operation_cancelled",
    RefreshStageStatus::Interrupted
)]
fn primary_failure_selects_the_stage_status(
    #[case] failure: StageFailure,
    #[case] code: &'static str,
    #[case] expected: RefreshStageStatus,
) {
    let mut stage = test_stage();
    let repository = "owner/repo".parse().expect("repository");

    stage.record_failure(&repository, failure);
    let report = stage.finish();

    assert_eq!(report.status, expected);
    assert_eq!(report.failure.expect("primary failure").code, code);
    assert_eq!(report.report.expect("stage report").len(), 1);
}

#[test]
fn later_failure_preserves_an_earlier_successful_repository() {
    let mut stage = test_stage();
    let first: RepositorySelector = "owner/first".parse().expect("first repository");
    let second = "owner/second".parse().expect("second repository");

    stage.record_success(&first, complete_report());
    stage.record_failure(&second, StageFailure::failed(missing_identity()));
    let report = stage.finish();
    let results = report.report.expect("stage report");

    assert_eq!(report.status, RefreshStageStatus::Partial);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].repository, first.as_url());
    assert!(results[0].report.is_some());
    assert!(results[1].failure.is_some());
}

#[test]
fn cancellation_does_not_replace_an_earlier_primary_failure_or_invent_an_attempt() {
    let mut stage = test_stage();
    let repository = "owner/repo".parse().expect("repository");

    stage.record_failure(&repository, StageFailure::failed(missing_identity()));
    stage.interrupt();
    let report = stage.finish();

    assert_eq!(report.status, RefreshStageStatus::Failed);
    assert_eq!(
        report.failure.expect("primary failure").code,
        "embedding_service_identity_missing"
    );
    assert_eq!(report.report.expect("attempted repositories").len(), 1);
}

/// Constructs unattempted stage state with normal graph policy and no service identity.
fn test_stage() -> ClusterStage<'static> {
    ClusterStage {
        identity: None,
        recipe: DocumentRecipe::OriginalBody,
        options: ClusterOptions::default(),
        results: Vec::new(),
        first_failure: None,
        has_partial_coverage: false,
    }
}

/// Constructs a complete generation report with explicit thread, edge, and membership counts.
fn complete_report() -> ClusterBuildReport {
    ClusterBuildReport {
        generation: ClusterGenerationResult {
            run_id: 1,
            cluster_count: 1,
            member_count: 2,
            retired_count: 0,
            complete_coverage: true,
        },
        eligible_threads: 2,
        vector_threads: 2,
        candidate_edges: 1,
    }
}
