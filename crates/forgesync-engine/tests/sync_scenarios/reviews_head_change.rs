//! # Review head freshness
//!
//! A changed pull-request head makes previously acquired reviews stale.
//! The second acquisition selects parent metadata only. The scenario checks that old review
//! membership remains available but cannot claim completeness for the new head.
//!
//! The real sync requests and operation calls stay in this scenario; fixture modules only
//! configure provider responses, construct clients, and read local state. No acquisition is hidden
//! in a test helper. Source head, family selection, and expected canonical state remain explicit.
//! This integration regression complements focused store ordering and finalization tests.

use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::sync::{SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fixture_archive::{
    remove_archive, review_coverage, review_members, review_thread_coverage,
    temporary_archive_path, thread_summary,
};
use super::fixture_issues::{clients_for, mount_open_issues, mount_repository};
use super::fixture_reviews::{
    mount_graphql_review_threads, mount_pull_request_metadata, mount_pull_reviews,
    pull_request_issue, pull_review, review_thread, review_thread_page,
};

#[tokio::test]
async fn changed_pull_request_head_marks_old_reviews_stale_without_refetching_them() {
    let server = MockServer::start().await;
    mount_repository(&server).await;
    mount_open_issues(&server, vec![pull_request_issue("2026-09-20T09:30:00Z")]).await;
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
    mount_graphql_review_threads(
        &server,
        review_thread_page(
            vec![review_thread("PRRT_reviewed_head", false)],
            false,
            None,
        ),
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
        include_reviews: true,
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
    assert_eq!(
        review_members(&archive, 18).await[0]
            .payload
            .commit_sha
            .as_ref()
            .unwrap()
            .as_str(),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );

    server.reset().await;
    mount_repository(&server).await;
    mount_open_issues(&server, vec![pull_request_issue("2026-09-21T09:30:00Z")]).await;
    mount_pull_request_metadata(&server, "cccccccccccccccccccccccccccccccccccccccc", false).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/18/reviews"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let clients = clients_for(&server, &selector);
    let request = SyncRequest {
        repositories: vec![selector],
        all: false,
        scope: SyncThreadScope::Open,
        include_comments: false,
        include_reviews: false,
        include_review_threads: false,
        parent_run: None,
    };
    let metadata_only = sync_repositories(
        &archive,
        &clients,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report");
    assert_eq!(metadata_only.outcome, OperationOutcome::Complete);
    assert_eq!(metadata_only.reviews_seen, 0);

    let summary = thread_summary(&archive, 18).await;
    assert!(matches!(
        review_coverage(&summary).state(),
        CoverageState::Complete { item_count: 1, .. }
    ));
    assert!(review_coverage(&summary).is_stale());
    assert!(matches!(
        review_thread_coverage(&summary).state(),
        CoverageState::Complete { item_count: 1, .. }
    ));
    assert!(review_thread_coverage(&summary).is_stale());
    assert_eq!(
        review_members(&archive, 18).await[0]
            .payload
            .commit_sha
            .as_ref()
            .unwrap()
            .as_str(),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
    let metadata = archive
        .child_family_members::<forgesync_core::content::PullRequestMetadata>(
            &summary.discussion.id,
            EvidenceFamily::PullRequestMetadata,
        )
        .await
        .expect("read current pull-request metadata");
    assert_eq!(
        metadata[0].payload.head.sha.as_str(),
        "cccccccccccccccccccccccccccccccccccccccc"
    );

    archive.close().await;
    remove_archive(&archive_path);
}
