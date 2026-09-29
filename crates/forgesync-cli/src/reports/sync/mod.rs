//! # Explain acquisition and refresh outcomes
//!
//! Sync and refresh summaries present counts, family failures, and stage statuses. Exit-code
//! helpers derive process results from structured outcomes rather than scattered command-specific
//! booleans.
//!
//! Partial success is expected when independent jobs fail. Reporting must preserve successful
//! committed work and point to failures that can be inspected or retried.
//!
//! Refresh presentation follows `selected` stage order and appends the remaining-work list. A
//! stage without a record is omitted; a record without a payload still shows its status and safe
//! failure. Named payload formatters handle acquisition, embedding, and cluster counts, while the
//! shared stage formatter keeps their status/detail/failure ordering consistent.

use std::process::ExitCode;

use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::refresh::{
    RefreshClusterRepository, RefreshEmbeddingReport, RefreshReport, RefreshStage,
    RefreshStageKind, RefreshStageStatus,
};
use forgesync_engine::sync::SyncReport;

/// Maps a structured workflow outcome to the process status contract.
pub fn outcome_exit_code(outcome: &OperationOutcome) -> ExitCode {
    match outcome {
        OperationOutcome::Complete => ExitCode::SUCCESS,
        OperationOutcome::Partial { .. } | OperationOutcome::Deferred { .. } => ExitCode::from(3),
        OperationOutcome::Interrupted { .. } => ExitCode::from(130),
        OperationOutcome::Failed { .. } => ExitCode::FAILURE,
    }
}

/// Formats durable sync counts and partial outcomes for a terminal reader.
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

/// Presents selected refresh stages and their independent outcomes.
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

/// Selects the stage's presentation, omitting absent stage records without fabricating an outcome.
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

/// Formats a stage's status, optional payload counts, and safe failure in the same order.
///
/// The stage already owns status/report/failure as one concept. A named payload formatter supplies
/// family-specific counts without copying those fields into a second presentation state type.
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

/// Presents acquisition scope and completed jobs from the retained sync report.
fn sync_details(report: &SyncReport) -> String {
    format!(
        "{} repositories, {}/{} jobs complete",
        report.repositories_selected, report.completed_jobs, report.total_jobs
    )
}

/// Presents document materialization and vector batch counts without replacing their failure rows.
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

/// Counts generated groups only from repository reports that produced a generation.
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

/// Derives one display status from a multi-stage refresh report.
pub fn refresh_report_status(report: &RefreshReport) -> RefreshStageStatus {
    match report.outcome {
        OperationOutcome::Complete => RefreshStageStatus::Complete,
        OperationOutcome::Interrupted { .. } => RefreshStageStatus::Interrupted,
        OperationOutcome::Failed { .. } => RefreshStageStatus::Failed,
        OperationOutcome::Deferred { .. } => RefreshStageStatus::Deferred,
        OperationOutcome::Partial { .. } => RefreshStageStatus::Partial,
    }
}

/// Returns the stable human label for a refresh stage.
pub fn refresh_stage_name(stage: RefreshStageKind) -> &'static str {
    match stage {
        RefreshStageKind::Sync => "sync",
        RefreshStageKind::Embeddings => "embeddings",
        RefreshStageKind::Clusters => "clusters",
    }
}

/// Returns the stable human label for a refresh stage status.
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
