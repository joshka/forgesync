//! # Recorded retry selection decoding
//!
//! The run ledger stores original request scope as JSON. These named cases protect the decoding
//! policy used when a failure has no family or explicit state key: absent or malformed facts never
//! enable additional acquisition, while valid original inclusion flags survive retry planning.
//!
//! Each case compares the complete decoded selection. Provider acquisition and failure resolution
//! are covered separately by the sync retry integration scenario, which verifies that selecting
//! comments does not acquire reviews or resolve their ledger failures.

use serde_json::json;

use super::RecordedSelection;
use crate::sync::SyncThreadScope;

#[rstest::rstest]
#[case::missing(
    json!({}),
    RecordedSelection { scope: SyncThreadScope::Default, comments: false, reviews: false, review_threads: false }
)]
#[case::malformed(
    json!({"thread_scope": 1, "include_comments": "true", "include_reviews": 1, "include_review_threads": []}),
    RecordedSelection { scope: SyncThreadScope::Default, comments: false, reviews: false, review_threads: false }
)]
#[case::all_requested(
    json!({"thread_scope": "all", "include_comments": true, "include_reviews": true, "include_review_threads": true}),
    RecordedSelection { scope: SyncThreadScope::All, comments: true, reviews: true, review_threads: true }
)]
#[case::closed_reviews(
    json!({"thread_scope": "closed", "include_comments": false, "include_reviews": true}),
    RecordedSelection { scope: SyncThreadScope::Closed, comments: false, reviews: true, review_threads: false }
)]
#[case::open_comments(
    json!({"thread_scope": "open", "include_comments": true}),
    RecordedSelection { scope: SyncThreadScope::Open, comments: true, reviews: false, review_threads: false }
)]
#[case::unknown_scope(
    json!({"thread_scope": "future", "include_review_threads": true}),
    RecordedSelection { scope: SyncThreadScope::Default, comments: false, reviews: false, review_threads: true }
)]
fn recorded_selection_retains_only_valid_request_facts(
    #[case] scope: serde_json::Value,
    #[case] expected: RecordedSelection,
) {
    let selection = RecordedSelection::from_scope(&scope);
    assert_eq!(selection, expected);
}
