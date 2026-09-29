//! Reviews workflow contracts.

use super::{
    Archive, CoverageState, EvidenceFamily, Mock, MockServer, OperationOutcome, RepositorySelector,
    ResponseTemplate, ReviewState, SyncThreadScope, body_string_contains, comment, comment_bodies,
    comment_coverage, json, method, mount_comments, mount_graphql_review_threads,
    mount_open_issues, mount_pull_request_metadata, mount_pull_reviews, mount_repository, path,
    pull_request_issue, pull_review, remove_archive, review_coverage, review_members,
    review_thread, review_thread_coverage, review_thread_members, review_thread_page,
    sync_once_with_families, temporary_archive_path, thread_summary,
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
    let initial = sync_once_with_families(
        &archive,
        &server,
        selector.clone(),
        SyncThreadScope::Open,
        true,
        true,
        false,
    )
    .await;
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
    let failed_refresh = sync_once_with_families(
        &archive,
        &server,
        selector,
        SyncThreadScope::Open,
        false,
        true,
        false,
    )
    .await;
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
    let initial = sync_once_with_families(
        &archive,
        &server,
        selector.clone(),
        SyncThreadScope::Open,
        false,
        true,
        true,
    )
    .await;
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
    let metadata_only = sync_once_with_families(
        &archive,
        &server,
        selector,
        SyncThreadScope::Open,
        false,
        false,
        false,
    )
    .await;
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
    let initial = sync_once_with_families(
        &archive,
        &server,
        selector.clone(),
        SyncThreadScope::Open,
        false,
        false,
        true,
    )
    .await;
    assert_eq!(initial.outcome, OperationOutcome::Complete);
    assert_eq!(initial.review_threads_seen, 2);
    assert_eq!(review_thread_members(&archive, 18).await.len(), 2);

    server.reset().await;
    mount_repository(&server).await;
    mount_open_issues(&server, vec![pull_request_issue("2026-09-21T09:30:00Z")]).await;
    mount_pull_request_metadata(&server, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", false).await;
    mount_graphql_review_threads(&server, review_thread_page(Vec::new(), false, None)).await;
    let removed = sync_once_with_families(
        &archive,
        &server,
        selector.clone(),
        SyncThreadScope::Open,
        false,
        false,
        true,
    )
    .await;
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
    let restored = sync_once_with_families(
        &archive,
        &server,
        selector,
        SyncThreadScope::Open,
        false,
        false,
        true,
    )
    .await;
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
    let initial = sync_once_with_families(
        &archive,
        &server,
        selector.clone(),
        SyncThreadScope::Open,
        false,
        false,
        true,
    )
    .await;
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
    let failed = sync_once_with_families(
        &archive,
        &server,
        selector,
        SyncThreadScope::Open,
        false,
        false,
        true,
    )
    .await;
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
