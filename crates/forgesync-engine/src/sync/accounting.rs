//! # Run-wide acquisition accounting and outcome selection
//!
//! `WorkSummary` accumulates counts after family jobs record their durable results. It is used by
//! parent-thread, comment, and pull-request jobs; it does not acquire provider data or write SQL.
//! `record_failure` classifies failures and retains the first diagnostic, while
//! `record_family_outcome` accounts for terminal family jobs. `finish_pending` includes interrupted
//! terminal jobs in remaining work, since a terminal ledger record does not mean satisfied scope.
//!
//! The coordinator asks `outcome` for the final run policy: cancellation takes precedence, mixed
//! success and failures are partial, exhausted rate-limit budgets can defer the entire run, and
//! failure without useful success retains its first diagnostic. The returned public report reloads
//! durable run/job/failure rows and combines them with these acquisition counts.

use forgesync_core::coverage::{DeferredReason, Failure, FailureKind};
use forgesync_core::outcome::OperationOutcome;

/// In-memory accounting accumulated after durable job and observation updates.
///
/// Terminal jobs include failures and interruptions; they are not synonymous with successful jobs.
/// Cancellation adds interrupted jobs back into pending work because their scope needs retry. The
/// final outcome uses these counters while the returned report reloads authoritative ledger rows.
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
    /// Provider pages whose acquisition was durably recorded.
    pub pages_completed: u64,
    /// Parent discussion rows received by recorded scans.
    pub threads_seen: u64,
    /// Discussion comments received by recorded scans.
    pub comments_seen: u64,
    /// Pull-request metadata records received by recorded acquisitions.
    pub pull_request_metadata_seen: u64,
    /// Review records received by recorded acquisitions.
    pub reviews_seen: u64,
    /// Review-thread records received by recorded acquisitions.
    pub review_threads_seen: u64,
    /// Whether cancellation stopped selected work before the run completed.
    pub interrupted: bool,
    /// Terminal interrupted jobs that must also count as remaining work.
    pub interrupted_jobs: u64,
    /// Remaining jobs calculated after interruption, including interrupted terminal jobs.
    pub pending_jobs: u64,
    /// First encountered failure used when the whole run fails without successful jobs.
    pub first_failure: Option<Failure>,
}

impl WorkSummary {
    /// Includes interrupted jobs in the remaining-work count when cancellation stopped the run.
    pub fn finish_pending(&mut self) {
        if self.interrupted {
            self.pending_jobs = self
                .total_jobs
                .saturating_sub(self.completed_jobs)
                .saturating_add(self.interrupted_jobs);
        }
    }

    /// Adds a finished family job to run accounting while retaining its first failure.
    pub fn record_family_outcome(&mut self, failure: Option<&Failure>, interrupted: bool) {
        self.completed_jobs = self.completed_jobs.saturating_add(1);
        if let Some(failure) = failure {
            self.record_failure(failure);
        }
        if interrupted {
            self.interrupted = true;
            self.interrupted_jobs = self.interrupted_jobs.saturating_add(1);
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

    /// Derives the final complete, partial, or interrupted run outcome.
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

#[cfg(test)]
#[path = "accounting_tests.rs"]
mod tests;
