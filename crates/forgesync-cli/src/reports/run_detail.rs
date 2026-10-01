//! One run's identity, jobs, and failures in store row order.
//!
//! A resolved failure stays visible in detail, marked with its recorded resolution.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_store::runs::{RunDetail, RunFailureRecord, SyncJobRecord};

use crate::reports::runs::{run_status_name, sync_job_status_name};
use crate::reports::threads::format_timestamp;

/// Shows the run heading, then its jobs and failures in store order.
pub fn run_detail_summary(detail: &RunDetail) -> String {
    let mut lines = vec![run_heading(detail), "Jobs:".to_owned()];
    lines.extend(detail.jobs.iter().map(job_row));
    lines.push("Failures:".to_owned());
    lines.extend(detail.failures.iter().map(failure_row));
    if detail.failures.is_empty() {
        lines.push("  None".to_owned());
    }
    lines.join("\n")
}

/// Identifies the run, its parent and start time, and its record counts.
fn run_heading(detail: &RunDetail) -> String {
    let run = &detail.run;
    let started = format_timestamp(run.started_at);
    let parent = run
        .parent_id
        .map(|parent| parent.get().to_string())
        .unwrap_or_else(|| "-".to_owned());
    format!(
        "Run {}: {}\nStarted: {}\nParent: {}\nJobs: {}\nFailures: {}",
        run.id.get(),
        run_status_name(run.status),
        started,
        parent,
        detail.jobs.len(),
        detail.failures.len()
    )
}

/// Shows one job's repository, family, scope, and committed page and item totals.
fn job_row(job: &SyncJobRecord) -> String {
    format!(
        "  {} {} [{}]: {} ({} items, {} pages)",
        job.repository.full_name,
        job.family.as_str(),
        job.scope_key,
        sync_job_status_name(job.status),
        job.items_committed,
        job.pages_completed
    )
}

/// Shows one failure's target and safe diagnostic, marking any recorded resolution.
fn failure_row(failure: &RunFailureRecord) -> String {
    let family = failure
        .family
        .map(EvidenceFamily::as_str)
        .unwrap_or("unassigned");
    let resolution = if failure.resolved_at.is_some() {
        " (resolved)"
    } else {
        ""
    };
    format!(
        "  {} {} [{}]: {}{}",
        failure.target, family, failure.scope_key, failure.failure.message, resolution
    )
}

#[cfg(test)]
mod tests {
    use forgesync_core::identity::RunId;
    use forgesync_core::timestamp::UtcTimestamp;
    use forgesync_store::runs::{RunDetail, RunRecord, RunStatus};

    use crate::reports::run_detail::run_detail_summary;

    #[test]
    fn empty_run_keeps_identity_and_explicit_no_failures_line() {
        let at = UtcTimestamp::parse("2026-09-29T00:00:00Z").expect("display timestamp");
        let detail = RunDetail {
            run: RunRecord {
                id: RunId::new(17).expect("display run ID"),
                parent_id: None,
                status: RunStatus::Complete,
                started_at: at,
                updated_at: at,
                finished_at: Some(at),
                scope: serde_json::json!({}),
                outcome: None,
            },
            jobs: Vec::new(),
            failures: Vec::new(),
        };
        assert_eq!(
            run_detail_summary(&detail),
            concat!(
                "Run 17: complete\nStarted: 2026-09-29T00:00:00Z\nParent: -\n",
                "Jobs: 0\nFailures: 0\nJobs:\nFailures:\n  None",
            )
        );
    }
}
