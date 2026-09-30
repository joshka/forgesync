//! # Partial review-thread isolation
//!
//! A partial GraphQL collection cannot replace the last complete review-thread membership.
//! The scenario first commits complete membership, then supplies a response whose continuation
//! cannot be completed. Canonical members survive while coverage records the partial attempt.
//!
//! The real sync requests and operation calls stay in this scenario; fixture modules only
//! configure provider responses, construct clients, and read local state. No acquisition is hidden
//! in a test helper. Source head, family selection, and expected canonical state remain explicit.
//! This integration regression complements focused store ordering and finalization tests.

use forgesync_core::coverage::CoverageState;
use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::sync::{SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_store::archive::Archive;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fixture_archive::{
    remove_archive, review_thread_coverage, review_thread_members, temporary_archive_path,
    thread_summary,
};
use super::fixture_issues::{clients_for, mount_open_issues, mount_repository};
use super::fixture_reviews::{
    mount_graphql_review_threads, mount_pull_request_metadata, pull_request_issue, review_thread,
    review_thread_page,
};

#[tokio::test]
async fn partial_graphql_review_thread_snapshot_keeps_last_complete_membership() {
    let server = MockServer::start().await;
    mount_repository(&server).await;
    mount_open_issues(&server, vec![pull_request_issue("2026-09-20T09:30:00Z")]).await;
    mount_pull_request_metadata(&server, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", false).await;
    mount_graphql_review_threads(
        &server,
        review_thread_page(vec![review_thread("PRRT_old", false)], false, None),
    )
    .await;

    let archive_path = temporary_archive_path();
    let archive = Archive::create(&archive_path)
        .await
        .expect("create archive");
    let selector = "owner/repo"
        .parse::<RepositorySelector>()
        .expect("selector");
    let clients = clients_for(&server, &selector);
    let request = SyncRequest {
        repositories: vec![selector.clone()],
        all: false,
        scope: SyncThreadScope::Open,
        include_comments: false,
        include_reviews: false,
        include_review_threads: true,
        parent_run: None,
    };
    let initial = sync_repositories(
        &archive,
        &clients,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report");
    assert_eq!(initial.outcome, OperationOutcome::Complete);
    let original_members = review_thread_members(&archive, 18).await;

    server.reset().await;
    mount_repository(&server).await;
    mount_open_issues(&server, vec![pull_request_issue("2026-09-21T09:30:00Z")]).await;
    mount_pull_request_metadata(&server, "cccccccccccccccccccccccccccccccccccccccc", false).await;
    mount_graphql_review_threads(
        &server,
        review_thread_page(Vec::new(), true, Some("thread-cursor-1")),
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("reviewThreads(first: 100"))
        .and(body_string_contains("thread-cursor-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"repository": {"pullRequest": {"reviewThreads": {
                "nodes": [review_thread("PRRT_partial", true)],
                "pageInfo": {"hasNextPage": false, "endCursor": null}
            }}}},
            "errors": [{"message": "Synthetic partial second page"}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let clients = clients_for(&server, &selector);
    let request = SyncRequest {
        repositories: vec![selector],
        all: false,
        scope: SyncThreadScope::Open,
        include_comments: false,
        include_reviews: false,
        include_review_threads: true,
        parent_run: None,
    };
    let failed = sync_repositories(
        &archive,
        &clients,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report");
    assert!(matches!(
        failed.outcome,
        OperationOutcome::Partial {
            failed_items: 1,
            ..
        }
    ));
    assert_eq!(review_thread_members(&archive, 18).await, original_members);
    let summary = thread_summary(&archive, 18).await;
    assert!(matches!(
        review_thread_coverage(&summary).state(),
        CoverageState::Incomplete { .. }
    ));
    assert!(review_thread_coverage(&summary).is_stale());

    archive.close().await;
    remove_archive(&archive_path);
}
