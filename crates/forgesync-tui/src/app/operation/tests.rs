//! # Single-writer display transitions
//!
//! These tests exercise operation presentation without starting background acquisition. They
//! protect the separation between a reserved writer generation and transient running state:
//! refusing another writer cannot change its label, stale completion cannot finish it, and a
//! completed generation cannot regain progress when a late message arrives.
//!
//! The app suite separately checks that writer failure restores keyboard-driven quitting. Here
//! each test follows one short transition sequence and compares the resulting display facts.

use forgesync_core::identity::RunId;
use forgesync_engine::sync::{SyncProgress, SyncProgressStatus};

use super::OperationDisplay;

#[test]
fn second_writer_does_not_replace_the_active_generation_or_label() {
    let mut operation = OperationDisplay::default();
    assert_eq!(operation.begin("sync"), Some(1));
    assert_eq!(operation.begin("refresh"), None);
    assert_eq!(operation.generation, 1);
    assert_eq!(operation.label(), Some("sync"));
    assert!(operation.busy());
}

#[test]
fn stale_completion_leaves_the_current_writer_running() {
    let mut operation = OperationDisplay::default();
    assert_eq!(operation.begin("sync"), Some(1));
    let status = operation.finish(0, Ok("old result".to_owned()));
    assert_eq!(status, None);
    assert!(operation.busy());
    assert_eq!(operation.label(), Some("sync"));
}

#[test]
fn failed_completion_clears_transient_state_and_preserves_the_generation() {
    let mut operation = OperationDisplay::default();
    assert_eq!(operation.begin("sync"), Some(1));
    let status = operation.finish(1, Err("archive unavailable".to_owned()));
    assert_eq!(status.as_deref(), Some("Failed: archive unavailable"));
    assert!(!operation.busy());
    assert_eq!(operation.label(), None);
    assert_eq!(operation.progress(), None);
    assert_eq!(operation.generation, 1);
}

#[test]
fn progress_after_completion_cannot_resurrect_the_finished_writer() {
    let mut operation = OperationDisplay::default();
    assert_eq!(operation.begin("sync"), Some(1));
    assert_eq!(
        operation.finish(1, Ok("done".to_owned())).as_deref(),
        Some("done")
    );
    let progress = SyncProgress {
        run_id: RunId::new(1).expect("positive run ID"),
        completed_jobs: 1,
        total_jobs: 2,
        threads_seen: 3,
        comments_seen: 4,
        pull_request_metadata_seen: 5,
        reviews_seen: 6,
        review_threads_seen: 7,
        repository: Some("owner/repo".to_owned()),
        status: SyncProgressStatus::InProgress,
    };
    operation.update_progress(1, progress);
    assert_eq!(operation.progress(), None);
    assert!(!operation.busy());
}
