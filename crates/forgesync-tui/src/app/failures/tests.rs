//! # Failed-run refresh and cursor bounds
//!
//! These direct transitions protect retained retry choices during refresh, current-error reporting,
//! empty-result cursor behavior, and stale-result rejection. The fixture is one explicit run
//! summary; no archive scan, provider activity, or retry execution is hidden in test setup.
//!
//! The full retry ledger and selected-family acquisition are covered by engine workflows. Here a
//! list refresh cannot finish another generation or leave a selected index outside new row bounds.

use forgesync_store::runs::RunStatus;
use rstest::{fixture, rstest};

use super::{FailureList, RunFailureSummary};

/// One safe retry choice with an error that the next refresh must clear.
#[fixture]
fn loaded_list() -> FailureList {
    FailureList {
        items: vec![RunFailureSummary {
            id: 23,
            status: RunStatus::Failed,
            entries: vec!["threads failed".to_owned()],
        }],
        error: Some("previous failure".to_owned()),
        ..Default::default()
    }
}

#[rstest]
fn refresh_retains_retry_choices_and_clears_the_previous_error(mut loaded_list: FailureList) {
    assert_eq!(loaded_list.begin(), 1);
    assert!(loaded_list.loading);
    assert_eq!(loaded_list.items[0].id, 23);
    assert_eq!(loaded_list.error, None);
}

#[rstest]
fn empty_replacement_clamps_the_cursor_to_its_safe_sentinel(mut loaded_list: FailureList) {
    loaded_list.selected = 4;
    assert_eq!(loaded_list.begin(), 1);
    assert_eq!(loaded_list.apply(1, Ok(Vec::new())), None);
    assert!(loaded_list.items.is_empty());
    assert_eq!(loaded_list.selected, 0);
    assert!(!loaded_list.loading);
}

#[rstest]
fn failed_refresh_preserves_retry_choices_and_reports_the_current_error(
    mut loaded_list: FailureList,
) {
    assert_eq!(loaded_list.begin(), 1);
    assert_eq!(
        loaded_list
            .apply(1, Err("read failed".to_owned()))
            .as_deref(),
        Some("read failed")
    );
    assert_eq!(loaded_list.items[0].id, 23);
    assert_eq!(loaded_list.error.as_deref(), Some("read failed"));
    assert!(!loaded_list.loading);
}

#[rstest]
fn stale_empty_result_cannot_replace_choices_or_finish_the_current_read(
    mut loaded_list: FailureList,
) {
    assert_eq!(loaded_list.begin(), 1);
    assert_eq!(loaded_list.begin(), 2);
    assert_eq!(loaded_list.apply(1, Ok(Vec::new())), None);
    assert_eq!(loaded_list.items[0].id, 23);
    assert_eq!(loaded_list.generation, 2);
    assert!(loaded_list.loading);
    assert_eq!(loaded_list.error, None);
}
