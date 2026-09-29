//! # Build bounded failure-list projections from the run ledger
//!
//! [`recent_failures`] reads the fifty newest run records and details at most twenty unfinished
//! runs. These are presentation bounds, not a complete retry inventory: older failures remain in
//! the archive and can still be inspected through the run commands. Completed runs do not consume
//! the detail bound; the selected unfinished runs retain the store's newest-first order.
//!
//! Each detail read is isolated. A missing or unreadable run becomes a summary with its original
//! identity and status, so another run's evidence remains available. Failure to read the initial
//! list fails the projection as a whole. No provider requests or durable changes occur here.
//!
//! [`RunFailureSummary`] owns conversion of detailed jobs and unresolved failure rows into safe
//! terminal entries. [`crate::query::reads`] attaches the panel generation and sends this
//! projection to [`crate::app::failures::FailureList`], which owns cache retention and retry
//! selection.

use forgesync_engine::error::EngineError;
use forgesync_engine::runs::{list_runs, show_run};
use forgesync_store::archive::Archive;
use forgesync_store::runs::{RunRecord, RunStatus};

use crate::app::failures::RunFailureSummary;

/// Caps ledger candidates so opening the failure view does not scan all archive history.
const RUNS_TO_SCAN: u32 = 50;
/// Caps detail reads after completed runs have been removed from the candidate list.
const RUNS_TO_DETAIL: usize = 20;

/// Reads recent unfinished runs without letting an individual detail failure hide other runs.
///
/// A list failure returns a safe error for the whole panel. A detail failure is represented in
/// that run's entries, retaining the run identity so its ledger can still be inspected or retried.
pub async fn recent_failures(archive: &Archive) -> Result<Vec<RunFailureSummary>, String> {
    let runs = list_runs(archive, RUNS_TO_SCAN)
        .await
        .map_err(|error| error.to_string())?;
    let mut summaries = Vec::new();
    for run in unfinished_runs(runs) {
        summaries.push(run_summary(archive, run).await);
    }
    Ok(summaries)
}

/// Preserves newest-first ledger order while selecting a bounded unfinished subset.
fn unfinished_runs(runs: Vec<RunRecord>) -> impl Iterator<Item = RunRecord> {
    runs.into_iter()
        .filter(|run| run.status != RunStatus::Complete)
        .take(RUNS_TO_DETAIL)
}

/// Projects one run or reports its isolated detail failure under the original identity.
async fn run_summary(archive: &Archive, run: RunRecord) -> RunFailureSummary {
    match show_run(archive, run.id).await {
        Ok(detail) => RunFailureSummary::from(detail),
        Err(error) => unavailable_summary(run, error),
    }
}

/// Keeps an unreadable ledger detail visible without manufacturing job or retry evidence.
fn unavailable_summary(run: RunRecord, error: EngineError) -> RunFailureSummary {
    RunFailureSummary {
        id: run.id.get(),
        status: run.status,
        entries: vec![format!("Could not load run detail: {error}")],
    }
}

#[cfg(test)]
mod tests;
