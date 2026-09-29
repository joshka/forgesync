//! Runs command presentation.

use forgesync_engine::runs::RetryReport;
use forgesync_store::runs::{RunDetail, RunRecord, RunStatus, SyncJobStatus};

use crate::reports::{family_name, sync_summary};

pub(crate) fn run_list_summary(runs: &Vec<RunRecord>) -> String {
    let mut lines = vec!["ID\tSTATUS\tSTARTED\tPARENT".to_owned()];
    for run in runs {
        lines.push(format!(
            "{}\t{}\t{}\t{}",
            run.id.get(),
            run_status_name(run.status),
            run.started_at
                .format_rfc3339()
                .unwrap_or_else(|_| "invalid timestamp".to_owned()),
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

pub(crate) fn run_detail_summary(detail: &RunDetail) -> String {
    let mut lines = vec![format!(
        "Run {}: {}\nStarted: {}\nParent: {}\nJobs: {}\nFailures: {}",
        detail.run.id.get(),
        run_status_name(detail.run.status),
        detail
            .run
            .started_at
            .format_rfc3339()
            .unwrap_or_else(|_| "invalid timestamp".to_owned()),
        detail
            .run
            .parent_id
            .map(|parent| parent.get().to_string())
            .unwrap_or_else(|| "-".to_owned()),
        detail.jobs.len(),
        detail.failures.len()
    )];
    lines.push("Jobs:".to_owned());
    lines.extend(detail.jobs.iter().map(|job| {
        format!(
            "  {} {} [{}]: {} ({} items, {} pages)",
            job.repository.full_name,
            family_name(job.family),
            job.scope_key,
            sync_job_status_name(job.status),
            job.items_committed,
            job.pages_completed
        )
    }));
    lines.push("Failures:".to_owned());
    lines.extend(detail.failures.iter().map(|failure| {
        format!(
            "  {} {} [{}]: {}{}",
            failure.target,
            failure.family.map(family_name).unwrap_or("unassigned"),
            failure.scope_key,
            failure.failure.message,
            if failure.resolved_at.is_some() {
                " (resolved)"
            } else {
                ""
            }
        )
    }));
    if detail.failures.is_empty() {
        lines.push("  None".to_owned());
    }
    lines.join("\n")
}

pub(crate) fn retry_summary(report: &RetryReport) -> String {
    let mut lines = vec![format!(
        "Retry of run {}: {} failure(s), {} sync run(s)",
        report.parent_run_id.get(),
        report.failure_ids.len(),
        report.runs.len()
    )];
    lines.extend(report.runs.iter().map(sync_summary));
    lines.join("\n")
}

pub(crate) fn run_status_name(status: RunStatus) -> &'static str {
    match status {
        RunStatus::InProgress => "in_progress",
        RunStatus::Complete => "complete",
        RunStatus::Partial => "partial",
        RunStatus::Failed => "failed",
        RunStatus::Interrupted => "interrupted",
        RunStatus::Deferred => "deferred",
    }
}

pub(crate) fn sync_job_status_name(status: SyncJobStatus) -> &'static str {
    match status {
        SyncJobStatus::Pending => "pending",
        SyncJobStatus::InProgress => "in_progress",
        SyncJobStatus::Complete => "complete",
        SyncJobStatus::Failed => "failed",
        SyncJobStatus::Deferred => "deferred",
        SyncJobStatus::Interrupted => "interrupted",
    }
}
