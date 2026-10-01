//! Recorded run-scope decoding: unreadable or absent facts never enable additional acquisition.

use serde_json::json;

use crate::sync::{RunScope, SyncThreadScope};

#[rstest::rstest]
#[case::missing(
    json!({}),
    RunScope::default()
)]
#[case::malformed(
    json!({"thread_scope": 1, "include_comments": "true", "include_reviews": 1, "include_review_threads": []}),
    RunScope::default()
)]
#[case::all_requested(
    json!({"repositories": [], "all": false, "thread_scope": "all", "include_comments": true, "include_reviews": true, "include_review_threads": true}),
    RunScope { thread_scope: SyncThreadScope::All, include_comments: true, include_reviews: true, include_review_threads: true, ..RunScope::default() }
)]
#[case::closed_reviews(
    json!({"repositories": ["https://github.com/owner/repo"], "all": false, "thread_scope": "closed", "include_comments": false, "include_reviews": true, "include_review_threads": false}),
    RunScope { repositories: vec!["https://github.com/owner/repo".to_owned()], thread_scope: SyncThreadScope::Closed, include_comments: false, include_reviews: true, include_review_threads: false, ..RunScope::default() }
)]
#[case::unknown_scope(
    json!({"thread_scope": "future", "include_review_threads": true}),
    RunScope::default()
)]
fn recorded_scope_decodes_only_valid_request_facts(
    #[case] scope: serde_json::Value,
    #[case] expected: RunScope,
) {
    let selection = RunScope::decode(&scope);
    assert_eq!(selection, expected);
}
