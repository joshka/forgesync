//! # Retry scenarios
//!
//! The initial workflow records failures for multiple child families on one pull request. Retry
//! planning selects only comments, so the next operation must leave reviews unresolved and make no
//! review-provider request. The original attempt remains inspectable in the durable run ledger.
//!
//! The selected failure and scope counts are checked before indexed field assertions. The retry run
//! names its original parent, and the stored comment failure points to that successful retry while
//! the review failure stays unresolved. Local ledger reads have separate result/presence failures.
//!
//! Provider setup may construct responses and clients, but retry planning and execution remain in
//! this linear scenario. The two phases stay together because family selection is defined by the
//! original recorded failures; this suite does not reconstruct a synthetic retry plan.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::runs::{plan_run_retry, run_retry};
use forgesync_engine::sync::{SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fixture_archive::{remove_archive, temporary_archive_path};
use super::fixture_issues::{
    clients_for, comment, mount_comments, mount_open_issues, mount_repository,
};
use super::fixture_reviews::{mount_pull_request_metadata, mount_pull_reviews, pull_request_issue};

#[tokio::test]
async fn retry_selects_one_family_and_leaves_other_failures_unresolved() {
    let server = MockServer::start().await;
    mount_repository(&server).await;
    mount_open_issues(&server, vec![pull_request_issue("2026-09-20T09:30:00Z")]).await;
    mount_pull_request_metadata(&server, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", false).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/18/comments"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    mount_pull_reviews(&server, 404, Vec::new()).await;

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
    let original = sync_repositories(
        &archive,
        &clients,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report");
    assert_eq!(original.failures.len(), 3, "{:?}", original.failures);
    assert!(
        original
            .failures
            .iter()
            .any(|failure| failure.family == Some(EvidenceFamily::Comments))
    );
    assert!(
        original
            .failures
            .iter()
            .any(|failure| failure.family == Some(EvidenceFamily::Reviews))
    );

    server.reset().await;
    mount_repository(&server).await;
    mount_open_issues(&server, vec![pull_request_issue("2026-09-20T09:30:00Z")]).await;
    mount_pull_request_metadata(&server, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", false).await;
    mount_comments(&server, 18, vec![comment(1801, "recovered comment")]).await;
    mount_pull_reviews(&server, 404, Vec::new()).await;

    let plan = plan_run_retry(&archive, original.run.id, &[EvidenceFamily::Comments])
        .await
        .expect("plan selected retry");
    assert_eq!(plan.failure_ids.len(), 1);
    assert_eq!(plan.scopes.len(), 1);
    assert!(plan.scopes[0].include_comments);
    assert!(!plan.scopes[0].include_reviews);
    let clients = clients_for(&server, &selector);
    let retry = run_retry(&archive, &clients, plan, &CancellationToken::new(), None)
        .await
        .expect("run selected retry");
    assert_eq!(retry.runs.len(), 1);
    assert_eq!(retry.runs[0].run.parent_id, Some(original.run.id));
    assert_eq!(retry.runs[0].outcome, OperationOutcome::Complete);

    let updated = archive
        .run_detail(original.run.id)
        .await
        .expect("read original run");
    let updated = updated.expect("original run exists");
    let comments = updated
        .failures
        .iter()
        .find(|failure| failure.family == Some(EvidenceFamily::Comments))
        .expect("comment failure");
    let reviews = updated
        .failures
        .iter()
        .find(|failure| failure.family == Some(EvidenceFamily::Reviews))
        .expect("review failure");
    assert!(comments.resolved_at.is_some());
    assert_eq!(comments.retry_run_id, Some(retry.runs[0].run.id));
    assert!(reviews.resolved_at.is_none());
    let requests = server.received_requests().await.expect("received requests");
    assert!(
        requests
            .iter()
            .all(|request| request.url.path() != "/api/v3/repos/owner/repo/pulls/18/reviews")
    );

    archive.close().await;
    remove_archive(&archive_path);
}
