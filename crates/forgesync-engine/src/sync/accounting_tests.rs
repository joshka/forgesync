//! # Direct scenarios for sync run outcome accounting
//!
//! These tests exercise the in-memory policy without provider or database fixtures. Durable job
//! writes are covered by the sync workflow integration suite; here the counters make each scenario
//! visible beside the outcome assertion.
//!
//! Complete, partial, deferred, failed, and interrupted outcomes have different meanings for retry.
//! In particular, interrupted terminal jobs remain pending work, and a later rate-limit failure
//! must not replace the first diagnostic. Each test selects one policy boundary rather than hiding
//! multiple scenarios behind a loop or setup helper.

use forgesync_core::coverage::{DeferredReason, Failure, FailureKind};
use forgesync_core::outcome::OperationOutcome;

use crate::sync::accounting::WorkSummary;

#[test]
fn completed_work_without_failures_is_complete() {
    let work = WorkSummary {
        total_jobs: 2,
        completed_jobs: 2,
        ..Default::default()
    };

    assert_eq!(work.outcome(), OperationOutcome::Complete);
}

#[test]
fn success_alongside_failed_and_deferred_jobs_is_partial() {
    let work = WorkSummary {
        total_jobs: 3,
        completed_jobs: 3,
        failed_jobs: 1,
        deferred_jobs: 1,
        ..Default::default()
    };

    assert_eq!(
        work.outcome(),
        OperationOutcome::Partial {
            failed_items: 1,
            deferred_items: 1,
        }
    );
}

#[test]
fn only_rate_limited_jobs_defer_the_run() {
    let work = WorkSummary {
        total_jobs: 2,
        completed_jobs: 2,
        deferred_jobs: 2,
        ..Default::default()
    };

    assert_eq!(
        work.outcome(),
        OperationOutcome::Deferred {
            reason: DeferredReason::RateLimitBudget,
        }
    );
}

#[test]
fn failure_accounting_retains_the_first_diagnostic() {
    let first = Failure {
        kind: FailureKind::ProviderResponse,
        message: "repository lookup failed".to_owned(),
    };
    let later = Failure {
        kind: FailureKind::RateLimited,
        message: "retry budget exhausted".to_owned(),
    };
    let mut work = WorkSummary {
        total_jobs: 2,
        completed_jobs: 2,
        ..Default::default()
    };

    work.record_failure(&first);
    work.record_failure(&later);

    assert_eq!(work.failed_jobs, 1);
    assert_eq!(work.deferred_jobs, 1);
    assert_eq!(work.outcome(), OperationOutcome::Failed { failure: first });
}

#[test]
fn interruption_includes_terminal_interrupted_jobs_in_remaining_work() {
    let mut work = WorkSummary {
        total_jobs: 4,
        completed_jobs: 2,
        failed_jobs: 1,
        interrupted_jobs: 1,
        interrupted: true,
        ..Default::default()
    };

    work.finish_pending();

    assert_eq!(work.pending_jobs, 3);
    assert_eq!(
        work.outcome(),
        OperationOutcome::Interrupted { pending_items: 3 }
    );
}
