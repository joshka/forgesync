//! Pull-request review fixtures.
use serde_json::json;
use wiremock::matchers::{body_string_contains, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fixture_issues::issue;

/// Builds pull request 18 as an issue-list entry with one advertised issue comment.
///
/// The pull_request link selects pull-request classification; head and merge metadata are supplied
/// by a separate endpoint fixture.
pub fn pull_request_issue(updated_at: &str) -> serde_json::Value {
    let mut issue = issue(1802, 18, "selected change");
    issue["updated_at"] = json!(updated_at);
    issue["comments"] = json!(1);
    issue["html_url"] = json!("https://github.com/owner/repo/pull/18");
    issue["pull_request"] = json!({
        "url": "https://api.github.com/repos/owner/repo/pulls/18"
    });
    issue
}

/// Installs head and merge facts for pull request 18 using the fixed main base.
///
/// The merged flag is a source fact, not a workflow selector. Review membership is mounted
/// separately.
pub async fn mount_pull_request_metadata(server: &MockServer, head_sha: &str, merged: bool) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/18"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "base": {
                "ref": "main",
                "sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "repo": { "id": 41, "full_name": "owner/repo" }
            },
            "head": {
                "ref": "topic",
                "sha": head_sha,
                "repo": { "id": 41, "full_name": "owner/repo" }
            },
            "draft": false,
            "merged": merged
        })))
        .mount(server)
        .await;
}

/// Installs the supplied status and review payload for pull request 18.
///
/// A successful response has no continuation link; non-success statuses exercise provider failure
/// handling without mutating archive state during fixture setup.
pub async fn mount_pull_reviews(server: &MockServer, status: u16, reviews: Vec<serde_json::Value>) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/18/reviews"))
        .and(query_param("per_page", "100"))
        .respond_with(ResponseTemplate::new(status).set_body_json(reviews))
        .mount(server)
        .await;
}

/// Installs the supplied first-page review-thread GraphQL response.
///
/// The matcher requires a null outer cursor; pagination and nested-comment behavior stay explicit
/// in the supplied response and any additional scenario mocks.
pub async fn mount_graphql_review_threads(server: &MockServer, response: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("reviewThreads(first: 100"))
        .and(body_string_contains("\"cursor\":null"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .mount(server)
        .await;
}

/// Wraps review-thread nodes in the provider response with explicit pagination facts.
///
/// The page flag and cursor are deliberately independent so scenarios can represent incomplete or
/// invalid provider responses; this constructor does not validate them.
pub fn review_thread_page(
    review_threads: Vec<serde_json::Value>,
    has_next_page: bool,
    end_cursor: Option<&str>,
) -> serde_json::Value {
    json!({
        "data": {"repository": {"pullRequest": {"reviewThreads": {
            "nodes": review_threads,
            "pageInfo": {"hasNextPage": has_next_page, "endCursor": end_cursor}
        }}}}
    })
}

/// Builds a review thread with explicit identity/resolution and a complete empty comment page.
///
/// Location is fixed incidental metadata. Resolution is an observed fact,
/// not an instruction to change the archived discussion.
pub fn review_thread(id: &str, is_resolved: bool) -> serde_json::Value {
    json!({
        "id": id,
        "isResolved": is_resolved,
        "isOutdated": false,
        "path": "src/lib.rs",
        "line": 42,
        "startLine": null,
        "comments": {
            "nodes": [],
            "pageInfo": {"hasNextPage": false, "endCursor": null}
        }
    })
}

/// Builds a changes-requested review tied to the supplied provider identity and head commit.
///
/// Fixed reviewer, body, and submission time leave head-context and membership assertions explicit.
pub fn pull_review(id: u64, commit_sha: &str) -> serde_json::Value {
    json!({
        "id": id,
        "state": "CHANGES_REQUESTED",
        "body": "Please revise this change.",
        "submitted_at": "2026-09-19T12:00:00Z",
        "commit_id": commit_sha,
        "user": { "id": 51, "login": "reviewer", "type": "User" },
        "author_association": "MEMBER"
    })
}
