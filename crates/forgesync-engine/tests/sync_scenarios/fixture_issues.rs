//! # REST discussion fixtures
//!
//! These constructors and endpoint installers describe owner/repo issue and comment responses.
//! `clients_for` routes the selected host to the local mock server without credentials; it does
//! not execute a workflow. Repository metadata and open issue collections are mounted separately.
//!
//! Payload builders keep provider identity, local discussion number, body, source time, and child
//! counts available to scenarios. Mounting changes mock server behavior only, never archive state.
//! Pagination and provider errors belong in the scenario when they are the behavior under test.
//! Pull-request metadata and review payloads have a separate owner in `fixture_reviews`.
use std::collections::HashMap;

use forgesync_core::identity::GitHubHost;
use forgesync_engine::reference::RepositorySelector;
use forgesync_github::transport::{GitHubClient, GitHubClientConfig};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Constructs credential-free clients routed to the fixture server for the selected host.
///
/// It neither mounts responses nor acquires data; scenarios call the real sync operation
/// explicitly.
pub fn clients_for(
    server: &MockServer,
    selector: &RepositorySelector,
) -> HashMap<GitHubHost, GitHubClient> {
    let api_base_url = format!("{}/api/v3/", server.uri())
        .parse()
        .expect("local API URL");
    let client =
        GitHubClient::new(GitHubClientConfig::new(api_base_url), None).expect("GitHub client");
    HashMap::from([(selector.host().clone(), client)])
}

/// Installs successful repository metadata for owner/repo, provider ID 41.
///
/// This only configures the mock server; the engine must acquire and register the repository.
pub async fn mount_repository(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 41,
            "name": "repo",
            "full_name": "owner/repo",
            "owner": { "login": "owner" },
            "default_branch": "main",
            "updated_at": "2026-09-20T12:00:00Z"
        })))
        .mount(server)
        .await;
}

/// Installs the supplied open-issue collection for owner/repo with no next-page link.
///
/// Scenarios needing pagination or failures mount their own response instead.
pub async fn mount_open_issues(server: &MockServer, issues: Vec<serde_json::Value>) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("state", "open"))
        .respond_with(ResponseTemplate::new(200).set_body_json(issues))
        .mount(server)
        .await;
}

/// Installs a complete successful comment response for the selected discussion number.
///
/// Payload identity and ordering are supplied by the scenario; this does not write the archive.
pub async fn mount_comments(server: &MockServer, number: u64, comments: Vec<serde_json::Value>) {
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v3/repos/owner/repo/issues/{number}/comments"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(comments))
        .mount(server)
        .await;
}

/// Builds an open issue with explicit source time and advertised comment count.
///
/// The count influences engine family freshness; it does not install or fabricate comment members.
pub fn issue_with_comment_count(
    id: u64,
    number: u64,
    title: &str,
    updated_at: &str,
    comments: u64,
) -> serde_json::Value {
    let mut issue = issue(id, number, title);
    issue["updated_at"] = json!(updated_at);
    issue["comments"] = json!(comments);
    issue
}

/// Builds an open issue with independent provider ID and repository-local number.
///
/// The fixed source time, empty child count, and absent body are incidental defaults that callers
/// may override before mounting the payload.
pub fn issue(id: u64, number: u64, title: &str) -> serde_json::Value {
    json!({
        "id": id,
        "number": number,
        "state": "open",
        "title": title,
        "body": null,
        "created_at": "2026-09-18T08:00:00Z",
        "updated_at": "2026-09-20T09:30:00Z",
        "comments": 0,
        "closed_at": null,
        "html_url": format!("https://github.com/owner/repo/issues/{number}"),
        "labels": [],
        "assignees": [],
        "user": { "login": "maintainer" }
    })
}

/// Builds an issue comment with explicit provider identity/body and fixed creation/source times.
///
/// The payload has no parent identity; its mounted endpoint supplies discussion context.
pub fn comment(id: u64, body: &str) -> serde_json::Value {
    json!({
        "id": id,
        "body": body,
        "created_at": "2026-09-19T08:00:00Z",
        "updated_at": "2026-09-19T08:00:00Z",
        "user": { "login": "reviewer" }
    })
}
