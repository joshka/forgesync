//! # Retry scenarios
//!
//! These cases use recorded run failures to select the work attempted again. They protect precise
//! scope: a failed family can be retried without repeating unrelated completed work. The previous
//! attempt remains inspectable in the run ledger.

use super::{
    Archive, CancellationToken, EvidenceFamily, Mock, MockServer, OperationOutcome,
    RepositorySelector, ResponseTemplate, SyncThreadScope, clients_for, comment, method,
    mount_comments, mount_open_issues, mount_pull_request_metadata, mount_pull_reviews,
    mount_repository, path, plan_run_retry, pull_request_issue, remove_archive, run_retry,
    sync_once_with_families, temporary_archive_path,
};

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
    let original = sync_once_with_families(
        &archive,
        &server,
        selector.clone(),
        SyncThreadScope::Open,
        true,
        true,
        false,
    )
    .await;
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
        .expect("read original run")
        .expect("original run exists");
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
    assert!(
        server
            .received_requests()
            .await
            .expect("received requests")
            .iter()
            .all(|request| request.url.path() != "/api/v3/repos/owner/repo/pulls/18/reviews")
    );

    archive.close().await;
    remove_archive(&archive_path);
}
