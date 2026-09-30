//! # Explain durable workflow attempts
//!
//! Run summaries present history, per-job detail, failure status, and retry outcomes. Status-name
//! helpers keep terminal labels aligned across list and detail views.
//!
//! A run is an attempt, while coverage describes acquired evidence. These reports should make the
//! difference clear when a job partially succeeded or a later retry completed the missing work.
//!
//! [`RetryCancellation`] represents interruption before a retry report exists; it carries no
//! fabricated run record. Completed retry reports retain their child-run outcomes instead. This
//! module adapts local ledger projections and does not choose which unresolved work to retry.

use forgesync_engine::runs::RetryReport;
use forgesync_store::runs::{RunRecord, RunStatus, SyncJobStatus};
use serde::Serialize;

use crate::reports::sync::sync_summary;

/// Safe interruption payload when retry stops before acquisition can produce a run report.
///
/// This command result contains a machine code and safe diagnostic text, not a fabricated ledger
/// run or acquisition outcome. The command supplies its interruption exit status when rendering.
#[derive(Debug, Serialize)]
pub struct RetryCancellation {
    /// Machine-readable interruption classification, independent of diagnostic wording.
    pub code: &'static str,
    /// Safe explanation of the pre-acquisition interruption for process output.
    pub message: String,
}

/// Formats recent durable run records in newest-first order.
pub fn run_list_summary(runs: &Vec<RunRecord>) -> String {
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

/// Summarizes the outcome of each scope attempted by an explicit retry.
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

/// Maps a durable run state to its human display label.
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

/// Maps a durable job state to its human display label.
pub fn sync_job_status_name(status: SyncJobStatus) -> &'static str {
    match status {
        SyncJobStatus::Pending => "pending",
        SyncJobStatus::InProgress => "in_progress",
        SyncJobStatus::Complete => "complete",
        SyncJobStatus::Failed => "failed",
        SyncJobStatus::Deferred => "deferred",
        SyncJobStatus::Interrupted => "interrupted",
    }
}

#[cfg(test)]
mod tests {
    //! # Retry interruption acknowledgment shape
    //!
    //! This case serializes the small command DTO independently of retry planning and execution.
    //! Its stable code and safe message remain explicit fields, without a run report or raw cause.
    //! Whole-value comparison catches field renaming, omission, or additional serialized data.
    //!
    //! The message is synthetic safe text; serialization does not sanitize arbitrary input.
    //! Process/workflow tests establish cancellation status and durable retry cleanup separately.
    //! Keeping this single projection case inline makes its representation easy to review beside
    //! the DTO without adding a fixture module or hiding a retry workflow.

    #[test]
    fn retry_interruption_json_retains_code_and_safe_message() {
        let output = crate::reports::runs::RetryCancellation {
            code: "operation_cancelled",
            message: "retry cancelled before acquisition".to_owned(),
        };

        let json = serde_json::to_value(output).expect("interruption JSON");

        assert_eq!(
            json,
            serde_json::json!({
                "code": "operation_cancelled", "message": "retry cancelled before acquisition",
            })
        );
    }
}
