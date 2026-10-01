//! Sync and refresh summaries and the outcome-to-exit mapping.
//!
//! Refresh presentation follows the report's `selected` stage order and appends remaining work. A
//! stage without a record is omitted; a record without a payload still shows its status and
//! failure.

use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::refresh::{
    RefreshClusterRepository, RefreshEmbeddingReport, RefreshReport, RefreshStage,
    RefreshStageKind, RefreshStageStatus,
};
use forgesync_engine::sync::SyncReport;

use crate::error::Exit;

/// Maps a workflow outcome to its process exit status.
pub fn outcome_exit_code(outcome: &OperationOutcome) -> Exit {
    match outcome {
        OperationOutcome::Complete => Exit::Success,
        OperationOutcome::Partial { .. } | OperationOutcome::Deferred { .. } => Exit::Partial,
        OperationOutcome::Interrupted { .. } => Exit::Interrupted,
        OperationOutcome::Failed { .. } => Exit::Failure,
    }
}

/// One-line sync outcome with job and evidence counts.
pub fn sync_summary(report: &SyncReport) -> String {
    let state = report.outcome.as_str();
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

/// Shows each selected stage in order, then any remaining work.
pub fn refresh_summary(report: &RefreshReport) -> String {
    let mut parts = report
        .selected
        .iter()
        .filter_map(|selected| refresh_stage_summary(report, *selected))
        .collect::<Vec<_>>();
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

/// Formats one selected stage, or returns `None` when it has no record.
fn refresh_stage_summary(report: &RefreshReport, selected: RefreshStageKind) -> Option<String> {
    match selected {
        RefreshStageKind::Sync => report
            .sync
            .as_ref()
            .map(|stage| stage_summary("sync", stage, sync_details)),
        RefreshStageKind::Embeddings => report
            .embeddings
            .as_ref()
            .map(|stage| stage_summary("embeddings", stage, embedding_details)),
        RefreshStageKind::Clusters => report.clusters.as_ref().map(|stage| {
            stage_summary("clusters", stage, |repositories| {
                cluster_details(repositories)
            })
        }),
    }
}

/// Formats a stage's status, optional payload counts, and failure in a consistent order.
fn stage_summary<T>(name: &str, stage: &RefreshStage<T>, details: fn(&T) -> String) -> String {
    let mut part = format!("{name} {}", refresh_status_name(stage.status));
    if let Some(report) = &stage.report {
        let detail = details(report);
        if !detail.is_empty() {
            part.push_str(": ");
            part.push_str(&detail);
        }
    }
    if let Some(failure) = &stage.failure {
        part.push_str("; ");
        part.push_str(&failure.message);
    }
    part
}

fn sync_details(report: &SyncReport) -> String {
    format!(
        "{} repositories, {}/{} jobs complete",
        report.repositories_selected, report.completed_jobs, report.total_jobs
    )
}

fn embedding_details(report: &RefreshEmbeddingReport) -> String {
    format!(
        "{} documents, {} chunks embedded, {} current, {} failed batches, {} document failures",
        report.embeddings.documents,
        report.embeddings.chunks_embedded,
        report.embeddings.chunks_skipped,
        report.embeddings.failed_batches.len(),
        report.document_failures.len()
    )
}

/// Counts groups only from repositories that produced a generation.
fn cluster_details(repositories: &[RefreshClusterRepository]) -> String {
    let generated = repositories
        .iter()
        .filter_map(|repository| repository.report.as_ref())
        .map(|cluster| cluster.generation.cluster_count)
        .sum::<u64>();
    format!(
        "{} repositories, {generated} generated groups",
        repositories.len()
    )
}

/// Derives one display status from the refresh report's overall outcome.
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

#[cfg(test)]
mod tests;
