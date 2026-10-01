use forgesync_core::coverage::{EvidenceFamily, Failure, FailureKind};
use forgesync_core::identity::RunId;
use forgesync_engine::error::EngineError;
use forgesync_store::runs::{RunDetail, RunFailureRecord, RunStatus, SyncJobRecord, SyncJobStatus};

use crate::app::test_data::{sample_repository, sample_run_record};
use crate::query::failures::{RunFailureSummary, unavailable_summary, unfinished_runs};

#[test]
fn completed_runs_are_omitted_without_reordering_unfinished_runs() {
    let first = sample_run_record();
    let mut complete = sample_run_record();
    complete.id = RunId::new(24).expect("run identity");
    complete.status = RunStatus::Complete;
    let mut last = sample_run_record();
    last.id = RunId::new(25).expect("run identity");
    last.status = RunStatus::Interrupted;

    let selected: Vec<_> = unfinished_runs(vec![first.clone(), complete, last.clone()]).collect();

    assert_eq!(selected, vec![first, last]);
}

#[test]
fn detail_selection_is_bounded_to_twenty_unfinished_runs() {
    let retained = sample_run_record();
    let mut boundary = sample_run_record();
    boundary.id = RunId::new(24).expect("boundary run identity");
    let mut omitted = sample_run_record();
    omitted.id = RunId::new(25).expect("omitted run identity");
    let mut candidates = vec![retained.clone(); 19];
    candidates.push(boundary.clone());
    candidates.push(omitted);
    let mut expected = vec![retained; 19];
    expected.push(boundary);

    let selected: Vec<_> = unfinished_runs(candidates).collect();

    assert_eq!(selected, expected);
}

#[test]
fn completed_runs_do_not_consume_the_detail_limit() {
    let unfinished = sample_run_record();
    let mut completed = sample_run_record();
    completed.id = RunId::new(24).expect("completed run identity");
    completed.status = RunStatus::Complete;
    let mut candidates = vec![completed; 20];
    candidates.push(unfinished.clone());

    let selected: Vec<_> = unfinished_runs(candidates).collect();

    assert_eq!(selected, vec![unfinished]);
}

#[test]
fn missing_detail_retains_original_run_identity_and_status() {
    let run = sample_run_record();
    let error = EngineError::RunMissing { id: 23 };
    let message = format!("Could not load run detail: {error}");

    let summary = unavailable_summary(run, error);

    assert_eq!(
        summary,
        RunFailureSummary {
            id: RunId::new(23).expect("run identity"),
            status: RunStatus::Failed,
            entries: vec![message],
        }
    );
}

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
    assert_eq!(summary.id.get(), 23);
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
    assert_eq!(summary.id.get(), 23);
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
