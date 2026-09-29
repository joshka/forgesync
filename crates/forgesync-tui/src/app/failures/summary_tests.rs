//! # Project unresolved ledger evidence into terminal entries
//!
//! The fixture constructors supply static durable records, without changing the archive. Each test
//! names the rows relevant to its behavior: completed jobs and resolved failures are omitted, while
//! unfinished jobs precede unresolved failures in their original order.

use forgesync_core::coverage::{EvidenceFamily, Failure, FailureKind};
use forgesync_store::runs::{RunDetail, RunFailureRecord, SyncJobRecord, SyncJobStatus};

use crate::app::failures::RunFailureSummary;
use crate::app::test_data::{sample_repository, sample_run_record};

#[test]
fn summary_keeps_unfinished_jobs_then_unresolved_failures() {
    let run = sample_run_record();
    let failed_job = sample_job();
    let mut complete_job = sample_job();
    complete_job.status = SyncJobStatus::Complete;
    let unresolved = sample_failure();
    let mut resolved = sample_failure();
    resolved.resolved_at = Some(run.updated_at);
    let detail = RunDetail {
        run,
        jobs: vec![complete_job, failed_job],
        failures: vec![resolved, unresolved],
    };

    let summary = RunFailureSummary::from(detail);

    assert_eq!(
        summary.entries,
        vec![
            "owner/repo / Threads: Failed".to_owned(),
            "owner/repo: could not acquire threads".to_owned(),
        ]
    );
    assert_eq!(summary.id, 23);
}

#[test]
fn run_without_unresolved_rows_has_no_manufactured_failure_entries() {
    let detail = RunDetail {
        run: sample_run_record(),
        jobs: Vec::new(),
        failures: Vec::new(),
    };

    let summary = RunFailureSummary::from(detail);

    assert!(summary.entries.is_empty());
    assert_eq!(summary.id, 23);
}

/// A static failed thread job; tests set its status directly when checking omission.
fn sample_job() -> SyncJobRecord {
    let run = sample_run_record();
    SyncJobRecord {
        id: 1,
        run_id: run.id,
        repository: sample_repository(),
        family: EvidenceFamily::Threads,
        scope_key: "default".to_owned(),
        status: SyncJobStatus::Failed,
        started_at: run.started_at,
        updated_at: run.updated_at,
        pages_completed: 0,
        items_committed: 0,
        failure: None,
    }
}

/// A static unresolved safe failure; tests set resolution explicitly when checking omission.
fn sample_failure() -> RunFailureRecord {
    RunFailureRecord {
        id: 2,
        target: "owner/repo".to_owned(),
        repository: None,
        family: Some(EvidenceFamily::Threads),
        thread_provider_id: None,
        thread_number: None,
        scope_key: "default".to_owned(),
        failure: Failure {
            kind: FailureKind::Network,
            message: "could not acquire threads".to_owned(),
        },
        created_at: sample_run_record().started_at,
        retry_count: 0,
        resolved_at: None,
        retry_run_id: None,
    }
}
