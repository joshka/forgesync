//! Bounded failure-list projections from the run ledger.
//!
//! These are presentation bounds, not a retry inventory: older failures stay in the archive and
//! remain visible through the run commands.

use forgesync_core::identity::RunId;
use forgesync_engine::error::EngineError;
use forgesync_engine::runs::{list_runs, show_run};
use forgesync_store::archive::Archive;
use forgesync_store::runs::{RunDetail, RunRecord, RunStatus, SyncJobStatus};

const RUNS_TO_SCAN: u32 = 50;
/// Applied after completed runs are removed, so they do not consume the bound.
const RUNS_TO_DETAIL: usize = 20;

/// One unfinished run and its safe unresolved-work lines.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunFailureSummary {
    pub id: RunId,
    pub status: RunStatus,
    pub entries: Vec<String>,
}

impl From<RunDetail> for RunFailureSummary {
    /// Lists unfinished jobs, then unresolved failures, each in ledger order.
    ///
    /// A job and its failure row may both appear because they explain different evidence; the
    /// engine reads the full ledger to decide what a retry covers.
    fn from(detail: RunDetail) -> Self {
        let jobs = detail
            .jobs
            .iter()
            .filter(|job| job.status != SyncJobStatus::Complete)
            .map(|job| {
                format!(
                    "{} / {:?}: {:?}",
                    job.repository.full_name, job.family, job.status
                )
            });
        let failures = detail
            .failures
            .iter()
            .filter(|failure| failure.resolved_at.is_none())
            .map(|failure| format!("{}: {}", failure.target, failure.failure.message));
        Self {
            id: detail.run.id,
            status: detail.run.status,
            entries: jobs.chain(failures).collect(),
        }
    }
}

/// Reads recent unfinished runs, newest first.
///
/// Only a failure to list runs fails the whole read; an unreadable run detail becomes that run's
/// only entry so other runs stay visible.
pub async fn recent_failures(archive: &Archive) -> Result<Vec<RunFailureSummary>, String> {
    let runs = list_runs(archive, RUNS_TO_SCAN)
        .await
        .map_err(|error| error.to_string())?;
    let mut summaries = Vec::new();
    for run in unfinished_runs(runs) {
        let summary = match show_run(archive, run.id).await {
            Ok(detail) => RunFailureSummary::from(detail),
            Err(error) => unavailable_summary(run, error),
        };
        summaries.push(summary);
    }
    Ok(summaries)
}

/// Unfinished runs in newest-first order, bounded after complete runs are skipped.
fn unfinished_runs(runs: Vec<RunRecord>) -> impl Iterator<Item = RunRecord> {
    runs.into_iter()
        .filter(|run| run.status != RunStatus::Complete)
        .take(RUNS_TO_DETAIL)
}

/// Keeps a run whose detail could not be read visible under its own identity.
fn unavailable_summary(run: RunRecord, error: EngineError) -> RunFailureSummary {
    RunFailureSummary {
        id: run.id,
        status: run.status,
        entries: vec![format!("Could not load run detail: {error}")],
    }
}

#[cfg(test)]
mod tests;
