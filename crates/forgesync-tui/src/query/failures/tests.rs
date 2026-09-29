//! # Bounded failure-list selection and isolated detail errors
//!
//! Static ledger records make the selected order and excluded complete run visible. These cases
//! cover presentation policy; engine/store tests separately cover durable ledger queries and retry.

use forgesync_core::identity::RunId;
use forgesync_engine::error::EngineError;
use forgesync_store::runs::RunStatus;

use crate::app::failures::RunFailureSummary;
use crate::app::test_data::sample_run_record;
use crate::query::failures::{unavailable_summary, unfinished_runs};

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
    let candidates = vec![sample_run_record(); 21];

    let selected: Vec<_> = unfinished_runs(candidates).collect();

    assert_eq!(selected.len(), 20);
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
            id: 23,
            status: RunStatus::Failed,
            entries: vec![message],
        }
    );
}
