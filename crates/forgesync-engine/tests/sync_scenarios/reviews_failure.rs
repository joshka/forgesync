//! # Review failure isolation
//!
//! A failed review refresh preserves unrelated comments and the last complete reviews.
//! The initial acquisition establishes both families. A later provider failure must leave their
//! canonical values intact while recording incomplete review evidence.
//!
//! The real sync requests and operation calls stay in this scenario; fixture modules only
//! configure provider responses, construct clients, and read local state. No acquisition is hidden
//! in a test helper. Source head, family selection, and expected canonical state remain explicit.
//! This integration regression complements focused store ordering and finalization tests.

use forgesync_core::content::ReviewState;
use forgesync_core::coverage::CoverageState;
use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::sync::{SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;
use wiremock::MockServer;

use super::fixture_archive::{
    comment_bodies, comment_coverage, remove_archive, review_coverage, review_members,
    temporary_archive_path, thread_summary,
};
use super::fixture_issues::{
    clients_for, comment, mount_comments, mount_open_issues, mount_repository,
};
use super::fixture_reviews::{
    mount_pull_request_metadata, mount_pull_reviews, pull_request_issue, pull_review,
};

#[tokio::test]
async fn failed_review_refresh_preserves_comments_and_last_complete_reviews() {
    let server = MockServer::start().await;
    mount_repository(&server).await;
    mount_open_issues(&server, vec![pull_request_issue("2026-09-20T09:30:00Z")]).await;
    mount_comments(&server, 18, vec![comment(1801, "existing issue comment")]).await;
    mount_pull_request_metadata(&server, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", false).await;
    mount_pull_reviews(
        &server,
        200,
        vec![pull_review(
            1811,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )],
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
        include_comments: true,
        include_reviews: true,
        include_review_threads: false,
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
    assert_eq!(initial.comments_seen, 1);
    assert_eq!(initial.reviews_seen, 1);
    let comments_before = comment_bodies(&archive, 18).await;
    let reviews_before = review_members(&archive, 18).await;
    assert_eq!(comments_before, ["existing issue comment"]);
    assert_eq!(reviews_before.len(), 1);

    server.reset().await;
    mount_repository(&server).await;
    mount_open_issues(&server, vec![pull_request_issue("2026-09-21T09:30:00Z")]).await;
    mount_pull_request_metadata(&server, "cccccccccccccccccccccccccccccccccccccccc", false).await;
    mount_pull_reviews(&server, 500, Vec::new()).await;
    let clients = clients_for(&server, &selector);
    let request = SyncRequest {
        repositories: vec![selector],
        all: false,
        scope: SyncThreadScope::Open,
        include_comments: false,
        include_reviews: true,
        include_review_threads: false,
        parent_run: None,
    };
    let failed_refresh = sync_repositories(
        &archive,
        &clients,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report");
    assert!(matches!(
        failed_refresh.outcome,
        OperationOutcome::Partial {
            failed_items: 1,
            ..
        }
    ));

    let summary = thread_summary(&archive, 18).await;
    assert_eq!(comment_bodies(&archive, 18).await, comments_before);
    assert!(comment_coverage(&summary).is_stale());
    assert!(matches!(
        review_coverage(&summary).state(),
        CoverageState::Incomplete { .. }
    ));
    assert!(review_coverage(&summary).is_stale());
    assert_eq!(review_members(&archive, 18).await, reviews_before);
    assert_eq!(
        review_members(&archive, 18).await[0].payload.state,
        ReviewState::ChangesRequested
    );

    archive.close().await;
    remove_archive(&archive_path);
}
