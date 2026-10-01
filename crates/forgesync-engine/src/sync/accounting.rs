//! Job and run accounting for one sync run.
//!
//! Terminal jobs include failures and interruptions. Cancellation adds interrupted jobs back into
//! pending work because their scope needs retry. The returned report reloads authoritative ledger
//! rows; these counters only select the run outcome and progress snapshots.

use forgesync_core::coverage::{DeferredReason, EvidenceFamily, Failure, FailureKind};
use forgesync_core::outcome::OperationOutcome;
use forgesync_store::runs::SyncJobStatus;

use crate::sync::SyncProgressStatus;

/// Run-wide counters accumulated after durable job updates.
#[derive(Default)]
pub struct WorkSummary {
    /// Initial repository/scope jobs plus pull-request family jobs added for nonempty scopes.
    pub total_jobs: u64,
    /// Jobs recorded as terminal, including failed, deferred, and interrupted jobs.
    pub completed_jobs: u64,
    /// Jobs ending with failures other than exhausted rate-limit budgets.
    pub failed_jobs: u64,
    /// Jobs deferred after the provider rate-limit retry budget expires.
    pub deferred_jobs: u64,
    pub pages_completed: u64,
    pub threads_seen: u64,
    pub comments_seen: u64,
    pub pull_request_metadata_seen: u64,
    pub reviews_seen: u64,
    pub review_threads_seen: u64,
    /// Whether cancellation stopped selected work before the run completed.
    pub interrupted: bool,
    /// Terminal interrupted jobs that must also count as remaining work.
    pub interrupted_jobs: u64,
    /// Remaining jobs calculated after interruption, including interrupted terminal jobs.
    pub pending_jobs: u64,
    /// First failure, used when the whole run fails without successful jobs.
    pub first_failure: Option<Failure>,
}

impl WorkSummary {
    /// Folds one terminal job into run accounting.
    pub fn finish_job(&mut self, family: EvidenceFamily, job: &JobProgress) {
        self.completed_jobs += 1;
        self.pages_completed += job.pages_completed;
        *self.seen(family) += job.items_received;
        if let Some(failure) = job.failure() {
            self.record_failure(failure);
        }
        if job.interrupted {
            self.interrupted = true;
            self.interrupted_jobs += 1;
        }
    }

    /// Received-record counter reported for `family`.
    fn seen(&mut self, family: EvidenceFamily) -> &mut u64 {
        match family {
            EvidenceFamily::Threads => &mut self.threads_seen,
            EvidenceFamily::Comments => &mut self.comments_seen,
            EvidenceFamily::PullRequestMetadata => &mut self.pull_request_metadata_seen,
            EvidenceFamily::Reviews => &mut self.reviews_seen,
            EvidenceFamily::ReviewThreads => &mut self.review_threads_seen,
        }
    }

    /// Adds one failure to aggregate work counts without losing its category.
    pub fn record_failure(&mut self, failure: &Failure) {
        if failure.kind == FailureKind::RateLimited {
            self.deferred_jobs += 1;
        } else {
            self.failed_jobs += 1;
        }
        if self.first_failure.is_none() {
            self.first_failure = Some(failure.clone());
        }
    }

    /// Includes interrupted jobs in the remaining-work count when cancellation stopped the run.
    pub fn finish_pending(&mut self) {
        if self.interrupted {
            self.pending_jobs = self
                .total_jobs
                .saturating_sub(self.completed_jobs)
                .saturating_add(self.interrupted_jobs);
        }
    }

    /// Derives the final run outcome; cancellation takes precedence.
    pub fn outcome(&self) -> OperationOutcome {
        if self.interrupted {
            return OperationOutcome::Interrupted {
                pending_items: self.pending_jobs,
            };
        }
        if self.failed_jobs > 0 || self.deferred_jobs > 0 {
            if self.completed_jobs > self.failed_jobs + self.deferred_jobs {
                return OperationOutcome::Partial {
                    failed_items: self.failed_jobs,
                    deferred_items: self.deferred_jobs,
                };
            }
            if self.failed_jobs == 0 {
                return OperationOutcome::Deferred {
                    reason: DeferredReason::RateLimitBudget,
                };
            }
            return OperationOutcome::Failed {
                failure: self.first_failure.clone().unwrap_or(Failure {
                    kind: FailureKind::ProviderResponse,
                    message: "sync failed before any repository completed".to_owned(),
                }),
            };
        }
        OperationOutcome::Complete
    }
}

/// Counts and failures accumulated by one durable job before its terminal write.
#[derive(Default)]
pub struct JobProgress {
    /// Pages durably written by this job's acquisitions.
    pub pages_completed: u64,
    /// Provider records received, including records not accepted as current membership.
    pub items_received: u64,
    /// Records accepted by the store.
    pub items_committed: u64,
    /// First non-rate-limited failure, preferred over deferral.
    pub hard_failure: Option<Failure>,
    /// First rate-limit failure.
    pub deferred_failure: Option<Failure>,
    /// Whether cancellation stopped any selected work.
    pub interrupted: bool,
}

impl JobProgress {
    /// Adds one discussion's family result while keeping the first failure of each category.
    pub fn add(&mut self, result: &FamilyResult) {
        self.pages_completed += result.pages_completed;
        self.items_received += result.items_received;
        self.items_committed += result.items_committed;
        self.interrupted |= result.interrupted;
        if let Some(failure) = &result.failure {
            self.add_failure(failure.clone());
        }
    }

    /// Keeps the first failure of its category (hard or rate-limited).
    pub fn add_failure(&mut self, failure: Failure) {
        let slot = if failure.kind == FailureKind::RateLimited {
            &mut self.deferred_failure
        } else {
            &mut self.hard_failure
        };
        slot.get_or_insert(failure);
    }

    /// Selects interruption, then hard failure, then deferral, then completion.
    pub fn status(&self) -> SyncJobStatus {
        if self.interrupted {
            SyncJobStatus::Interrupted
        } else if self.hard_failure.is_some() {
            SyncJobStatus::Failed
        } else if self.deferred_failure.is_some() {
            SyncJobStatus::Deferred
        } else {
            SyncJobStatus::Complete
        }
    }

    /// Failure attributed to the terminal status; interrupted jobs carry none.
    pub fn failure(&self) -> Option<&Failure> {
        if self.interrupted {
            None
        } else {
            self.hard_failure
                .as_ref()
                .or(self.deferred_failure.as_ref())
        }
    }
}

/// Acquisition evidence for one discussion's family.
///
/// Received and committed counts differ when stale or incomplete evidence cannot replace current
/// membership. Pages count every durably staged page, including those of incomplete attempts.
#[derive(Default)]
pub struct FamilyResult {
    pub pages_completed: u64,
    pub items_received: u64,
    pub items_committed: u64,
    pub failure: Option<Failure>,
    pub interrupted: bool,
}

/// Maps a terminal job status to the progress state shown to callers.
pub fn progress_status(status: SyncJobStatus) -> SyncProgressStatus {
    match status {
        SyncJobStatus::Complete => SyncProgressStatus::Complete,
        SyncJobStatus::Interrupted => SyncProgressStatus::Interrupted,
        SyncJobStatus::Deferred => SyncProgressStatus::Deferred,
        SyncJobStatus::InProgress | SyncJobStatus::Failed => SyncProgressStatus::Failed,
    }
}

#[cfg(test)]
#[path = "accounting_tests.rs"]
mod tests;
