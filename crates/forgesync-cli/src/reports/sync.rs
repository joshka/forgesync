//! Sync command presentation.

use std::process::ExitCode;

use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::refresh::{RefreshReport, RefreshStageKind, RefreshStageStatus};
use forgesync_engine::sync::SyncReport;

pub fn outcome_exit_code(outcome: &OperationOutcome) -> ExitCode {
    match outcome {
        OperationOutcome::Complete => ExitCode::SUCCESS,
        OperationOutcome::Partial { .. } | OperationOutcome::Deferred { .. } => ExitCode::from(3),
        OperationOutcome::Interrupted { .. } => ExitCode::from(130),
        OperationOutcome::Failed { .. } => ExitCode::FAILURE,
    }
}

pub fn sync_summary(report: &SyncReport) -> String {
    let state = match report.outcome {
        OperationOutcome::Complete => "complete",
        OperationOutcome::Partial { .. } => "partial",
        OperationOutcome::Deferred { .. } => "deferred",
        OperationOutcome::Interrupted { .. } => "interrupted",
        OperationOutcome::Failed { .. } => "failed",
    };
    format!(
        "Sync {state}: {} repositories, {}/{} jobs complete, {} failed, {} deferred, {} pages, {} threads, {} comments, {} PRs, {} reviews, {} review threads",
        report.repositories_selected,
        report.completed_jobs,
        report.total_jobs,
        report.failed_jobs,
        report.deferred_jobs,
        report.pages_completed,
        report.threads_seen,
        report.comments_seen,
        report.pull_request_metadata_seen,
        report.reviews_seen,
        report.review_threads_seen
    )
}

pub fn refresh_summary(report: &RefreshReport) -> String {
    let mut parts = Vec::new();
    for selected in &report.selected {
        let (status, detail, failure) = match selected {
            RefreshStageKind::Sync => {
                let Some(stage) = &report.sync else {
                    continue;
                };
                let detail = stage
                    .report
                    .as_ref()
                    .map(|sync| {
                        format!(
                            "{} repositories, {}/{} jobs complete",
                            sync.repositories_selected, sync.completed_jobs, sync.total_jobs
                        )
                    })
                    .unwrap_or_default();
                (stage.status, detail, stage.failure.as_ref())
            }
            RefreshStageKind::Embeddings => {
                let Some(stage) = &report.embeddings else {
                    continue;
                };
                let detail = stage
                    .report
                    .as_ref()
                    .map(|embedding| {
                        format!(
                            "{} documents, {} chunks embedded, {} current, {} failed batches, {} document failures",
                            embedding.embeddings.documents,
                            embedding.embeddings.chunks_embedded,
                            embedding.embeddings.chunks_skipped,
                            embedding.embeddings.failed_batches.len(),
                            embedding.document_failures.len()
                        )
                    })
                    .unwrap_or_default();
                (stage.status, detail, stage.failure.as_ref())
            }
            RefreshStageKind::Clusters => {
                let Some(stage) = &report.clusters else {
                    continue;
                };
                let detail = stage
                    .report
                    .as_ref()
                    .map(|repositories| {
                        let generated = repositories
                            .iter()
                            .filter_map(|repository| repository.report.as_ref())
                            .map(|cluster| cluster.generation.cluster_count)
                            .sum::<u64>();
                        format!(
                            "{} repositories, {generated} generated groups",
                            repositories.len()
                        )
                    })
                    .unwrap_or_default();
                (stage.status, detail, stage.failure.as_ref())
            }
        };
        let name = refresh_stage_name(*selected);
        let mut part = format!("{name} {}", refresh_status_name(status));
        if !detail.is_empty() {
            part.push_str(": ");
            part.push_str(&detail);
        }
        if let Some(failure) = failure {
            part.push_str("; ");
            part.push_str(&failure.message);
        }
        parts.push(part);
    }
    if !report.remaining.is_empty() {
        let remaining = report
            .remaining
            .iter()
            .map(|stage| refresh_stage_name(*stage))
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("remaining: {remaining}"));
    }
    format!(
        "Refresh {}: {}",
        refresh_status_name(refresh_report_status(report)),
        parts.join("; ")
    )
}

pub fn refresh_report_status(report: &RefreshReport) -> RefreshStageStatus {
    match report.outcome {
        OperationOutcome::Complete => RefreshStageStatus::Complete,
        OperationOutcome::Interrupted { .. } => RefreshStageStatus::Interrupted,
        OperationOutcome::Failed { .. } => RefreshStageStatus::Failed,
        OperationOutcome::Deferred { .. } => RefreshStageStatus::Deferred,
        OperationOutcome::Partial { .. } => RefreshStageStatus::Partial,
    }
}

pub fn refresh_stage_name(stage: RefreshStageKind) -> &'static str {
    match stage {
        RefreshStageKind::Sync => "sync",
        RefreshStageKind::Embeddings => "embeddings",
        RefreshStageKind::Clusters => "clusters",
    }
}

pub fn refresh_status_name(status: RefreshStageStatus) -> &'static str {
    match status {
        RefreshStageStatus::Complete => "complete",
        RefreshStageStatus::Partial => "partial",
        RefreshStageStatus::Failed => "failed",
        RefreshStageStatus::Interrupted => "interrupted",
        RefreshStageStatus::Deferred => "deferred",
    }
}
