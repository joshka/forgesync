//! Run history, job status, and retry summaries.
//!
//! A run is an attempt, while coverage describes acquired evidence; a partially successful job or
//! a later retry that completed the missing work should stay distinguishable.

use forgesync_engine::runs::RetryReport;
use forgesync_store::runs::{RunRecord, RunStatus, SyncJobStatus};

use crate::reports::sync::sync_summary;
use crate::reports::threads::format_timestamp;

/// Tabulates run records in the order supplied.
pub fn run_list_summary(runs: &Vec<RunRecord>) -> String {
    let mut lines = vec!["ID\tSTATUS\tSTARTED\tPARENT".to_owned()];
    for run in runs {
        lines.push(format!(
            "{}\t{}\t{}\t{}",
            run.id.get(),
            run_status_name(run.status),
            format_timestamp(run.started_at),
            run.parent_id
                .map(|parent| parent.get().to_string())
                .unwrap_or_else(|| "-".to_owned())
        ));
    }
    if runs.is_empty() {
        lines.push("No runs recorded.".to_owned());
    }
    lines.join("\n")
}

/// Summarizes the retry, then each child sync run it started.
pub fn retry_summary(report: &RetryReport) -> String {
    let mut lines = vec![format!(
        "Retry of run {}: {} failure(s), {} sync run(s)",
        report.parent_run_id.get(),
        report.failure_ids.len(),
        report.runs.len()
    )];
    lines.extend(report.runs.iter().map(sync_summary));
    lines.join("\n")
}

pub fn run_status_name(status: RunStatus) -> &'static str {
    match status {
        RunStatus::InProgress => "in_progress",
        RunStatus::Complete => "complete",
        RunStatus::Partial => "partial",
        RunStatus::Failed => "failed",
        RunStatus::Interrupted => "interrupted",
        RunStatus::Deferred => "deferred",
    }
}

pub fn sync_job_status_name(status: SyncJobStatus) -> &'static str {
    match status {
        SyncJobStatus::InProgress => "in_progress",
        SyncJobStatus::Complete => "complete",
        SyncJobStatus::Failed => "failed",
        SyncJobStatus::Deferred => "deferred",
        SyncJobStatus::Interrupted => "interrupted",
    }
}
