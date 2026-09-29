//! # Review-thread collection shape
//!
//! These tests cover nested review comments, thread resolution state, and complete pagination
//! results. Review threads have a separate family boundary from reviews and parent pull-request
//! metadata. A normalization change should preserve comment membership and completeness so the
//! store does not commit a misleading canonical family. The fixture makes provider nesting
//! explicit without asking readers to infer it from DTO declarations.

use forgesync_core::content::Repository;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadNumber};
use forgesync_core::provider_data::ProviderData;
use reqwest::Url;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fetch_review_thread_page;
use crate::transport::{GitHubClient, GitHubClientConfig};

fn scope() -> (Repository, forgesync_core::identity::ThreadId) {
    let repository_id = RepositoryId::new(
        GitHubHost::parse("ghe.example.test").expect("host"),
        ProviderId::new("41").expect("repository ID"),
    );
    let repository = Repository {
        id: repository_id.clone(),
        owner: "fixture-lab".to_owned(),
        name: "archive-demo".to_owned(),
        full_name: "fixture-lab/archive-demo".to_owned(),
        default_branch: Some("main".to_owned()),
        updated_at: None,
        provider_data: ProviderData::new(),
    };
    let thread = forgesync_core::identity::ThreadId::new(
        repository_id,
        ProviderId::new("1802").expect("thread ID"),
        ThreadNumber::new(18).expect("number"),
    );
    (repository, thread)
}

fn review_comment(id: &str, created_at: &str) -> serde_json::Value {
    json!({
        "id": id,
        "databaseId": 3001,
        "body": "Synthetic review-thread comment.",
        "author": { "login": "reviewer", "__typename": "User" },
        "path": "src/archive.rs",
        "diffHunk": "@@ -1 +1 @@",
        "createdAt": created_at,
        "updatedAt": null,
        "url": "https://github.example.test/pull/18#discussion_r3001",
        "pullRequestReview": { "id": "PRR_fixture_1811" }
    })
}

#[tokio::test]
async fn review_thread_pages_include_complete_nested_comments_and_resolution_state() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/enterprise/api/graphql"))
        .and(body_string_contains("reviewThreads(first: 100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"repository": {"pullRequest": {"reviewThreads": {
                "nodes": [{
                    "id": "PRRT_fixture_1",
                    "isResolved": false,
                    "isOutdated": false,
                    "path": "src/archive.rs",
                    "line": 42,
                    "startLine": 40,
                    "viewerCanResolve": true,
                    "comments": {
                        "nodes": [review_comment("PRRC_fixture_1", "2026-09-19T16:00:00Z")],
                        "pageInfo": {"hasNextPage": true, "endCursor": "comment-cursor-1"}
                    }
                }],
                "pageInfo": {"hasNextPage": true, "endCursor": "thread-cursor-1"}
            }}}}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/enterprise/api/graphql"))
        .and(body_string_contains("node(id: $threadID)"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"node": {"comments": {
                "nodes": [review_comment("PRRC_fixture_2", "2026-09-20T08:00:00Z")],
                "pageInfo": {"hasNextPage": false, "endCursor": null}
            }}}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client = GitHubClient::new(
        GitHubClientConfig::new(
            Url::parse(&format!("{}/enterprise/api/v3/", server.uri())).expect("base URL"),
        ),
        None,
    )
    .expect("client");
    let (repository, thread) = scope();
    let head = forgesync_core::identity::CommitSha::new("a".repeat(40)).expect("head SHA");
    let page = fetch_review_thread_page(
        &client,
        &repository,
        &thread,
        &head,
        None,
        &CancellationToken::new(),
    )
    .await
    .expect("review thread page");

    assert_eq!(
        page.next_cursor.expect("next cursor").as_str(),
        "thread-cursor-1"
    );
    assert_eq!(page.review_threads.len(), 1);
    let review_thread = &page.review_threads[0];
    assert_eq!(review_thread.id.provider_id().as_str(), "PRRT_fixture_1");
    assert!(!review_thread.is_resolved);
    assert!(!review_thread.is_outdated);
    assert_eq!(review_thread.head_sha, head);
    assert_eq!(review_thread.line, Some(42));
    assert_eq!(review_thread.comments.len(), 2);
    assert_eq!(
        review_thread.comments[0]
            .review_id
            .as_ref()
            .unwrap()
            .provider_id()
            .as_str(),
        "PRR_fixture_1811"
    );
    assert_eq!(
        review_thread.comments[0].provider_data.get("databaseId"),
        Some(&json!(3001))
    );
    assert_eq!(
        review_thread.provider_data.get("startLine"),
        Some(&json!(40))
    );
}

#[tokio::test]
async fn partial_graphql_errors_fail_even_when_data_is_present() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("reviewThreads(first: 100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"repository": {"pullRequest": {"reviewThreads": {
                "nodes": [], "pageInfo": {"hasNextPage": true, "endCursor": "next"}
            }}}},
            "errors": [{"message": "partial result"}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client = GitHubClient::new(
        GitHubClientConfig::new(Url::parse(&format!("{}/api/v3/", server.uri())).expect("URL")),
        None,
    )
    .expect("client");
    let (repository, thread) = scope();
    let head = forgesync_core::identity::CommitSha::new("a".repeat(40)).expect("head SHA");
    let error = fetch_review_thread_page(
        &client,
        &repository,
        &thread,
        &head,
        None,
        &CancellationToken::new(),
    )
    .await
    .expect_err("partial GraphQL result must fail");
    assert_eq!(error, crate::error::GitHubError::GraphqlErrors { count: 1 });
}

#[tokio::test]
async fn nested_partial_graphql_errors_prevent_a_complete_thread_page() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("reviewThreads(first: 100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"repository": {"pullRequest": {"reviewThreads": {
                "nodes": [{
                    "id": "PRRT_fixture_1",
                    "isResolved": false,
                    "isOutdated": false,
                    "comments": {
                        "nodes": [],
                        "pageInfo": {"hasNextPage": true, "endCursor": "comment-cursor-1"}
                    }
                }],
                "pageInfo": {"hasNextPage": false, "endCursor": null}
            }}}}
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("node(id: $threadID)"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"node": {"comments": {
                "nodes": [{"id": "PRRC_partial", "body": "partial", "createdAt": "2026-09-19T16:00:00Z"}],
                "pageInfo": {"hasNextPage": false, "endCursor": null}
            }}}
            , "errors": [{"message": "nested partial result"}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client = GitHubClient::new(
        GitHubClientConfig::new(Url::parse(&format!("{}/api/v3/", server.uri())).expect("URL")),
        None,
    )
    .expect("client");
    let (repository, thread) = scope();
    let head = forgesync_core::identity::CommitSha::new("a".repeat(40)).expect("head SHA");
    let error = fetch_review_thread_page(
        &client,
        &repository,
        &thread,
        &head,
        None,
        &CancellationToken::new(),
    )
    .await
    .expect_err("nested partial GraphQL result must fail");
    assert_eq!(error, crate::error::GitHubError::GraphqlErrors { count: 1 });
}

#[tokio::test]
async fn missing_graphql_cursor_is_rejected_as_incomplete_pagination() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("reviewThreads(first: 100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"repository": {"pullRequest": {"reviewThreads": {
                "nodes": [], "pageInfo": {"hasNextPage": true, "endCursor": null}
            }}}}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let client = GitHubClient::new(
        GitHubClientConfig::new(Url::parse(&format!("{}/api/v3/", server.uri())).expect("URL")),
        None,
    )
    .expect("client");
    let (repository, thread) = scope();
    let head = forgesync_core::identity::CommitSha::new("a".repeat(40)).expect("head SHA");
    let error = fetch_review_thread_page(
        &client,
        &repository,
        &thread,
        &head,
        None,
        &CancellationToken::new(),
    )
    .await
    .expect_err("missing cursor must not claim complete pagination");
    assert_eq!(error, crate::error::GitHubError::InvalidPaginationLink);
}
