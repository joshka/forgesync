//! # Explain one durable run's jobs and failures
//!
//! `run_detail_summary` presents a loaded store projection in identity, job, and failure order.
//! It does not retry, resolve, or infer coverage from a run outcome. The broader run-list and retry
//! summaries remain in `reports::runs`, alongside their shared status-label queries.
//!
//! `run_heading` identifies the attempt and its parent, start time, and selected record counts.
//! Job rows retain repository/family/scope and committed item/page totals. Failure rows retain
//! their target and safe diagnostic, including failures without an assigned family. A resolved
//! marker describes the ledger's recorded resolution, rather than hiding historical failures from
//! detail.
//!
//! Traversal preserves the store's row order. Empty failures receive an explicit `None` line;
//! empty jobs keep the heading alone. Missing parent, missing family, and invalid timestamp each
//! retain their existing distinct fallback. The command owns JSON and process-result policy.

use forgesync_store::runs::{RunDetail, RunFailureRecord, SyncJobRecord};

use crate::reports::runs::{run_status_name, sync_job_status_name};
use crate::reports::threads::family_name;

/// Shows the attempt followed by its ordered job and failure projections.
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

/// Identifies one attempt and counts its loaded ledger records, without inferring evidence
/// coverage.
fn run_heading(detail: &RunDetail) -> String {
    let run = &detail.run;
    let started = run
        .started_at
        .format_rfc3339()
        .unwrap_or_else(|_| "invalid timestamp".to_owned());
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

/// Shows committed work for a repository/family/scope without flattening its durable state.
fn job_row(job: &SyncJobRecord) -> String {
    format!(
        "  {} {} [{}]: {} ({} items, {} pages)",
        job.repository.full_name,
        family_name(job.family),
        job.scope_key,
        sync_job_status_name(job.status),
        job.items_committed,
        job.pages_completed
    )
}

/// Keeps safe failure text and optional recorded resolution visible for one ledger scope.
fn failure_row(failure: &RunFailureRecord) -> String {
    let family = failure.family.map(family_name).unwrap_or("unassigned");
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
    //! An empty attempt still identifies its parent policy and both ledger sections explicitly.

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
