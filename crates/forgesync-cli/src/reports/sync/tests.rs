//! # Refresh status and selected-stage presentation
//!
//! These cases construct stage records directly, without acquisition fixtures. They verify status
//! and failure rendering when no payload exists, selected-stage order, omission of absent records,
//! and the separate remaining-work suffix. Count formatting consumes existing engine reports and
//! is independent of archive access or service configuration.
//!
//! Selected order is supplied by the report, not recomputed from execution dependencies. A missing
//! stage record is omitted from details but can still appear in remaining work. The partial payload
//! case keeps successful counts before its diagnostic suffix; the renderer does not retry work or
//! derive completeness from counts. Engine tests own outcome calculation and stage scheduling.

use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::embeddings::EmbeddingReport;
use forgesync_engine::refresh::{
    RefreshEmbeddingReport, RefreshReport, RefreshStage, RefreshStageFailure, RefreshStageKind,
    RefreshStageStatus,
};
use forgesync_engine::sync::SyncReport;

use crate::reports::sync::{embedding_details, refresh_summary, stage_summary, sync_details};

#[rstest::rstest]
#[case::complete(RefreshStageStatus::Complete, None, "sync complete")]
#[case::failed(
    RefreshStageStatus::Failed,
    Some(RefreshStageFailure { code: "unavailable", message: "credentials unavailable".to_owned() }),
    "sync failed; credentials unavailable"
)]
fn stage_without_payload_retains_status_and_failure(
    #[case] status: RefreshStageStatus,
    #[case] failure: Option<RefreshStageFailure>,
    #[case] expected: &str,
) {
    let stage = RefreshStage::<SyncReport> {
        status,
        report: None,
        failure,
    };

    let summary = stage_summary("sync", &stage, sync_details);

    assert_eq!(summary, expected);
}

#[test]
fn selected_stages_keep_order_and_remaining_work_stays_visible() {
    let report = RefreshReport {
        selected: vec![
            RefreshStageKind::Embeddings,
            RefreshStageKind::Sync,
            RefreshStageKind::Clusters,
        ],
        sync: Some(RefreshStage {
            status: RefreshStageStatus::Complete,
            report: None,
            failure: None,
        }),
        embeddings: Some(RefreshStage {
            status: RefreshStageStatus::Failed,
            report: None,
            failure: Some(RefreshStageFailure {
                code: "service_unavailable",
                message: "service unavailable".to_owned(),
            }),
        }),
        clusters: None,
        remaining: vec![RefreshStageKind::Clusters],
        outcome: OperationOutcome::Partial {
            failed_items: 1,
            deferred_items: 0,
        },
    };

    let summary = refresh_summary(&report);

    assert_eq!(
        summary,
        "Refresh partial: embeddings failed; service unavailable; sync complete; remaining: clusters"
    );
}

#[test]
fn partial_embedding_payload_retains_counts_before_stage_failure() {
    let report = RefreshEmbeddingReport {
        embeddings: EmbeddingReport {
            documents: 2,
            chunks_embedded: 3,
            chunks_skipped: 4,
            ..Default::default()
        },
        documents_materialized: 2,
        document_failures: Vec::new(),
    };
    let stage = RefreshStage {
        status: RefreshStageStatus::Partial,
        report: Some(report),
        failure: Some(RefreshStageFailure {
            code: "repository_missing",
            message: "some repositories unavailable".to_owned(),
        }),
    };

    let summary = stage_summary("embeddings", &stage, embedding_details);

    assert_eq!(
        summary,
        "embeddings partial: 2 documents, 3 chunks embedded, 4 current, 0 failed batches, 0 document failures; some repositories unavailable"
    );
}
