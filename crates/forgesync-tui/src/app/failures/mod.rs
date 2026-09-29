//! # Failed-run list and retry selection
//!
//! [`FailureList`] owns loaded run summaries, the highlighted row, and its independent refresh
//! generation. [`RunFailureSummary`] is the safe presentation projection supplied by query tasks;
//! the engine and store retain the full job/failure ledger and determine retry eligibility.
//! Its `From<RunDetail>` conversion formats unfinished jobs and unresolved failure rows in ledger
//! order. Query failure selection supplies bounded recent candidates and isolates unreadable
//! details.
//!
//! Beginning a refresh retains rows while clearing the previous error. Success replaces rows and
//! clamps selection to their bounds. Failure retains the prior rows but exposes a safe error;
//! rendering gives that error precedence over stale list content.
//!
//! Input requests retry for the currently highlighted run, not for every displayed failure entry.
//! This owner only manages read state and cursor validity. Query task ownership still controls
//! acquisition, cancellation, and refreshing the archive after a writer completes.

use forgesync_store::runs::{RunDetail, RunStatus, SyncJobStatus};

/// Loaded retry choices and the pending read that may replace them.
#[derive(Debug, Default)]
pub struct FailureList {
    /// Most recently loaded run summaries, in query order.
    pub items: Vec<RunFailureSummary>,
    /// Highlighted item; zero also represents an empty list and must be checked against `items`.
    pub selected: usize,
    /// Most recently started failed-run read, used to reject stale replies.
    pub generation: u64,
    /// Whether the current read is pending; prior rows are retained during refresh.
    pub loading: bool,
    /// Safe current read failure, absent during a new read or after successful replacement.
    pub error: Option<String>,
}

/// Safe run-list projection keeping retry selection separate from the complete failure ledger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunFailureSummary {
    /// Archive-local run identity used when requesting a selected retry.
    pub id: u64,
    /// Durable run state displayed beside its failure summaries.
    pub status: RunStatus,
    /// Safe human-readable job and failure summaries; never raw provider payloads.
    pub entries: Vec<String>,
}

impl From<RunDetail> for RunFailureSummary {
    /// Formats unfinished jobs followed by unresolved ledger failures, retaining ledger order.
    ///
    /// Completed jobs and resolved failures are omitted. A job and its failure row may both be
    /// shown because they explain different evidence; entries do not determine retry eligibility.
    /// The engine reads the full ledger when a retry is requested.
    fn from(detail: RunDetail) -> Self {
        let mut entries: Vec<String> = detail
            .jobs
            .iter()
            .filter(|job| job.status != SyncJobStatus::Complete)
            .map(|job| {
                format!(
                    "{} / {:?}: {:?}",
                    job.repository.full_name, job.family, job.status
                )
            })
            .collect();
        entries.extend(
            detail
                .failures
                .iter()
                .filter(|failure| failure.resolved_at.is_none())
                .map(|failure| format!("{}: {}", failure.target, failure.failure.message)),
        );
        Self {
            id: detail.run.id.get(),
            status: detail.run.status,
            entries,
        }
    }
}

impl FailureList {
    /// Starts a refresh without discarding the currently loaded retry choices.
    pub fn begin(&mut self) -> u64 {
        self.generation += 1;
        self.loading = true;
        self.error = None;
        self.generation
    }

    /// Applies only the current generation, returning its safe failure to the app's status line.
    pub fn apply(
        &mut self,
        generation: u64,
        result: Result<Vec<RunFailureSummary>, String>,
    ) -> Option<String> {
        if generation != self.generation {
            return None;
        }
        self.loading = false;
        match result {
            Ok(items) => self.replace(items),
            Err(error) => self.fail(error),
        }
    }

    /// Replaces choices and clamps the selected row, including the empty-list sentinel at zero.
    fn replace(&mut self, items: Vec<RunFailureSummary>) -> Option<String> {
        self.items = items;
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
        self.error = None;
        None
    }

    /// Retains prior choices while exposing a read failure to both the view and status line.
    fn fail(&mut self, error: String) -> Option<String> {
        self.error = Some(error.clone());
        Some(error)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod summary_tests;
