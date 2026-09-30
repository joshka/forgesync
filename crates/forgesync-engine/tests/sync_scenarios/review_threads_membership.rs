//! # Complete review-thread membership
//!
//! Complete snapshots can remove and later restore canonical review-thread membership.
//! Three explicit acquisitions establish initial members, a complete empty collection, and a
//! later restored member. Each transition must replace membership only after complete evidence.
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
async fn complete_review_thread_snapshots_remove_and_restore_current_membership() {
    let server = MockServer::start().await;
    mount_repository(&server).await;
    mount_open_issues(&server, vec![pull_request_issue("2026-09-20T09:30:00Z")]).await;
    mount_pull_request_metadata(&server, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", false).await;
    mount_graphql_review_threads(
        &server,
        review_thread_page(
            vec![review_thread("PRRT_old", false)],
            true,
            Some("thread-cursor-1"),
        ),
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("reviewThreads(first: 100"))
        .and(body_string_contains("thread-cursor-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(review_thread_page(
            vec![review_thread("PRRT_page_two", false)],
            false,
            None,
        )))
        .expect(1)
        .mount(&server)
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
    assert_eq!(initial.review_threads_seen, 2);
    assert_eq!(review_thread_members(&archive, 18).await.len(), 2);

    server.reset().await;
    mount_repository(&server).await;
    mount_open_issues(&server, vec![pull_request_issue("2026-09-21T09:30:00Z")]).await;
    mount_pull_request_metadata(&server, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", false).await;
    mount_graphql_review_threads(&server, review_thread_page(Vec::new(), false, None)).await;
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
    let removed = sync_repositories(
        &archive,
        &clients,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report");
    assert_eq!(removed.outcome, OperationOutcome::Complete);
    assert!(review_thread_members(&archive, 18).await.is_empty());
    assert!(matches!(
        review_thread_coverage(&thread_summary(&archive, 18).await).state(),
        CoverageState::Complete { item_count: 0, .. }
    ));

    server.reset().await;
    mount_repository(&server).await;
    mount_open_issues(&server, vec![pull_request_issue("2026-09-22T09:30:00Z")]).await;
    mount_pull_request_metadata(&server, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", false).await;
    mount_graphql_review_threads(
        &server,
        review_thread_page(vec![review_thread("PRRT_restored", true)], false, None),
    )
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
    let restored = sync_repositories(
        &archive,
        &clients,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report");
    assert_eq!(restored.outcome, OperationOutcome::Complete);
    let members = review_thread_members(&archive, 18).await;
    assert_eq!(members.len(), 1);
    assert_eq!(
        members[0].payload.id.provider_id().as_str(),
        "PRRT_restored"
    );
    assert!(members[0].payload.is_resolved);

    archive.close().await;
    remove_archive(&archive_path);
}
