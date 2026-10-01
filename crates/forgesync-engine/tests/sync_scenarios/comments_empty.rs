//! Complete empty membership and failed empty acquisition remain distinct.

use forgesync_core::coverage::CoverageState;
use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::sync::{SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_store::archive::Archive;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fixture_archive::{
    comment_coverage, remove_archive, temporary_archive_path, thread_reference,
};
use super::fixture_issues::{
    clients_for, comment, issue_with_comment_count, mount_comments, mount_open_issues,
    mount_repository,
};

#[tokio::test]
async fn complete_empty_comments_replace_membership_but_incomplete_empty_does_not() {
    let server = MockServer::start().await;
    mount_repository(&server).await;
    mount_open_issues(
        &server,
        vec![
            issue_with_comment_count(91, 11, "first", "2026-09-20T09:30:00Z", 1),
            issue_with_comment_count(92, 12, "second", "2026-09-20T09:30:00Z", 1),
        ],
    )
    .await;
    mount_comments(&server, 11, vec![comment(1101, "removed")]).await;
    mount_comments(&server, 12, vec![comment(1201, "preserved")]).await;

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
        include_reviews: false,
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
    let initial_first = archive
        .thread_detail(&thread_reference(11))
        .await
        .expect("read original first membership");
    let initial_second = archive
        .thread_detail(&thread_reference(12))
        .await
        .expect("read original second membership");
    assert_eq!(initial_first.comments.len(), 1);
    assert_eq!(initial_first.comments[0].payload.body, "removed");
    assert_eq!(initial_second.comments.len(), 1);
    assert_eq!(initial_second.comments[0].payload.body, "preserved");

    server.reset().await;
    mount_repository(&server).await;
    mount_open_issues(
        &server,
        vec![
            issue_with_comment_count(91, 11, "first", "2026-09-21T10:00:00Z", 0),
            issue_with_comment_count(92, 12, "second", "2026-09-21T10:00:00Z", 0),
        ],
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/11/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/12/comments"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    let clients = clients_for(&server, &selector);
    let request = SyncRequest {
        repositories: vec![selector],
        all: false,
        scope: SyncThreadScope::Open,
        include_comments: true,
        include_reviews: false,
        include_review_threads: false,
        parent_run: None,
    };
    let partial = sync_repositories(
        &archive,
        &clients,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report");
    assert!(matches!(
        partial.outcome,
        OperationOutcome::Partial {
            failed_items: 1,
            ..
        }
    ));
    let completed = archive
        .thread_detail(&thread_reference(11))
        .await
        .expect("read completed empty membership");
    let failed = archive
        .thread_detail(&thread_reference(12))
        .await
        .expect("read retained failed membership");
    assert!(completed.comments.is_empty());
    assert_eq!(failed.comments, initial_second.comments);
    assert!(matches!(
        comment_coverage(&completed.summary).state(),
        CoverageState::Complete { item_count: 0, .. }
    ));
    assert!(matches!(
        comment_coverage(&failed.summary).state(),
        CoverageState::Incomplete {
            received_items: 0,
            ..
        }
    ));

    archive.close().await;
    remove_archive(&archive_path);
}
