use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use forgesync_core::{
    Comment, CoverageState, EvidenceFamily, FailureKind, GitHubHost, OperationOutcome, Review,
    ReviewState, ReviewThread, UtcTimestamp,
};
use forgesync_engine::{
    EngineError, RepositorySelector, SyncRequest, SyncThreadScope, plan_run_retry, run_retry,
    sync_repositories,
};
use forgesync_github::{GitHubClient, GitHubClientConfig};
use forgesync_store::{Archive, SyncJobStatus, ThreadQuery, ThreadSort, ThreadStateFilter};
use serde_json::json;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{body_string_contains, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

#[tokio::test]
async fn interrupted_page_replay_keeps_committed_threads_without_duplicates() {
    let server = MockServer::start().await;
    mount_repository(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("state", "open"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Link", "<?page=2>; rel=\"next\"")
                .set_body_json(json!([issue(91, 11, "page one")])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
        .mount(&server)
        .await;

    let archive_path = temporary_archive_path();
    let archive = Archive::create(&archive_path)
        .await
        .expect("create archive");
    let selector = "owner/repo"
        .parse::<RepositorySelector>()
        .expect("selector");
    let cancellation = CancellationToken::new();
    let task_cancellation = cancellation.clone();
    let host = selector.host().clone();
    let api_base_url = format!("{}/api/v3/", server.uri())
        .parse()
        .expect("local API URL");
    let client =
        GitHubClient::new(GitHubClientConfig::new(api_base_url), None).expect("GitHub client");
    let clients = HashMap::from([(host, client)]);
    let request = SyncRequest {
        repositories: vec![selector.clone()],
        all: false,
        scope: SyncThreadScope::Open,
        include_comments: false,
        include_reviews: false,
        include_review_threads: false,
        parent_run: None,
    };
    let sync_task = tokio::spawn(async move {
        let result =
            sync_repositories(&archive, &clients, &request, &task_cancellation, None).await;
        (archive, result)
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let requests = server.received_requests().await.unwrap_or_default();
            if requests.iter().any(|request| {
                request.url.path() == "/api/v3/repos/owner/repo/issues"
                    && request
                        .url
                        .query()
                        .is_some_and(|query| query.contains("page=2"))
            }) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("second page request starts");
    cancellation.cancel();
    let (archive, first_result) = sync_task.await.expect("sync task");
    let first = first_result.expect("interrupted sync report");
    assert_eq!(
        first.outcome,
        OperationOutcome::Interrupted { pending_items: 1 }
    );
    assert_eq!(first.jobs[0].status, SyncJobStatus::Interrupted);
    assert_eq!(first.pages_completed, 1);
    assert_eq!(first.threads_seen, 1);
    assert_eq!(thread_count(&archive).await, 1);

    server.reset().await;
    mount_repository(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("state", "open"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Link", "<?page=2>; rel=\"next\"")
                .set_body_json(json!([issue(91, 11, "page one")])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([issue(92, 12, "page two")])))
        .mount(&server)
        .await;

    let resumed = sync_once(&archive, &server, selector, SyncThreadScope::Open).await;
    assert_eq!(resumed.outcome, OperationOutcome::Complete);
    assert_eq!(resumed.pages_completed, 2);
    assert_eq!(resumed.threads_seen, 2);
    assert_eq!(thread_count(&archive).await, 2);

    archive.close().await;
    remove_archive(&archive_path);
}

#[tokio::test]
async fn closed_sweep_keeps_its_watermark_on_failure_and_retries_from_overlap() {
    let server = MockServer::start().await;
    mount_repository(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("state", "closed"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;

    let archive_path = temporary_archive_path();
    let archive = Archive::create(&archive_path)
        .await
        .expect("create archive");
    let selector = "owner/repo"
        .parse::<RepositorySelector>()
        .expect("selector");
    let initial = sync_once(&archive, &server, selector.clone(), SyncThreadScope::Closed).await;
    assert_eq!(initial.outcome, OperationOutcome::Complete);
    let repository = archive
        .find_repository(&GitHubHost::parse("github.com").unwrap(), "owner", "repo")
        .await
        .expect("find repository")
        .expect("registered repository");
    let original_watermark = archive
        .closed_sweep_watermark(&repository.id)
        .await
        .expect("read successful watermark")
        .expect("watermark exists");

    // Move the checkpoint back to model an archive that was offline for a long interval.
    let historical_watermark = UtcTimestamp::parse("2025-01-01T00:00:00Z").expect("timestamp");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&archive_path)
                .foreign_keys(true),
        )
        .await
        .expect("open archive for test checkpoint adjustment");
    sqlx::query(
        "UPDATE repository_checkpoints SET watermark_us = ? WHERE repository_id = (SELECT id FROM repositories WHERE host = 'github.com' AND provider_id = '41') AND checkpoint = 'closed_sweep'",
    )
    .bind(historical_watermark.unix_microseconds())
    .execute(&pool)
    .await
    .expect("move closed watermark into the past");
    pool.close().await;

    server.reset().await;
    mount_repository(&server).await;
    let since = "2024-12-31T00:00:00Z";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("state", "closed"))
        .and(query_param("since", since))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let failed = sync_once(&archive, &server, selector.clone(), SyncThreadScope::Closed).await;
    assert!(matches!(failed.outcome, OperationOutcome::Failed { .. }));
    assert_eq!(
        archive
            .closed_sweep_watermark(&repository.id)
            .await
            .expect("read unchanged watermark"),
        Some(historical_watermark)
    );

    server.reset().await;
    mount_repository(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("state", "closed"))
        .and(query_param("since", since))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let retried = sync_once(&archive, &server, selector, SyncThreadScope::Closed).await;
    assert_eq!(retried.outcome, OperationOutcome::Complete);
    let advanced_watermark = archive
        .closed_sweep_watermark(&repository.id)
        .await
        .expect("read advanced watermark")
        .expect("watermark exists");
    assert!(advanced_watermark > historical_watermark);
    assert!(advanced_watermark > original_watermark);

    archive.close().await;
    remove_archive(&archive_path);
}

#[tokio::test]
async fn comments_keep_sibling_success_and_retry_only_stale_threads() {
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
    mount_comments(&server, 11, vec![comment(1101, "old first")]).await;
    mount_comments(&server, 12, vec![comment(1201, "old second")]).await;

    let archive_path = temporary_archive_path();
    let archive = Archive::create(&archive_path)
        .await
        .expect("create archive");
    let selector = "owner/repo"
        .parse::<RepositorySelector>()
        .expect("selector");
    let initial = sync_once_with_comments(
        &archive,
        &server,
        selector.clone(),
        SyncThreadScope::Open,
        true,
    )
    .await;
    assert_eq!(initial.outcome, OperationOutcome::Complete);
    assert_eq!(initial.comments_seen, 2);
    assert_eq!(initial.jobs.len(), 2);

    server.reset().await;
    mount_repository(&server).await;
    mount_open_issues(
        &server,
        vec![
            issue_with_comment_count(91, 11, "first", "2026-09-20T09:30:00Z", 1),
            issue_with_comment_count(92, 12, "second", "2026-09-21T10:00:00Z", 2),
        ],
    )
    .await;
    let parent_refresh = sync_once_with_comments(
        &archive,
        &server,
        selector.clone(),
        SyncThreadScope::Open,
        false,
    )
    .await;
    assert_eq!(parent_refresh.outcome, OperationOutcome::Complete);
    let stale_summary = thread_summary(&archive, 12).await;
    let stale_coverage = comment_coverage(&stale_summary);
    assert!(stale_coverage.is_stale());

    server.reset().await;
    mount_repository(&server).await;
    mount_open_issues(
        &server,
        vec![
            issue_with_comment_count(91, 11, "first", "2026-09-20T09:30:00Z", 1),
            issue_with_comment_count(92, 12, "second", "2026-09-21T10:00:00Z", 2),
        ],
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/11/comments"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/12/comments"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/12/comments"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Link", "<?page=2>; rel=\"next\"")
                .set_body_json(json!([comment(1202, "partial replacement")])),
        )
        .expect(1)
        .mount(&server)
        .await;
    let partial = sync_once_with_comments(
        &archive,
        &server,
        selector.clone(),
        SyncThreadScope::Open,
        true,
    )
    .await;
    assert!(matches!(partial.outcome, OperationOutcome::Partial { .. }));
    assert_eq!(partial.failures.len(), 1);
    assert_eq!(partial.failures[0].thread_number, Some(12));
    assert_eq!(comment_bodies(&archive, 11).await, ["old first"]);
    assert_eq!(comment_bodies(&archive, 12).await, ["old second"]);
    let failed_summary = thread_summary(&archive, 12).await;
    let incomplete = comment_coverage(&failed_summary);
    assert!(matches!(
        incomplete.state(),
        CoverageState::Incomplete {
            received_items: 1,
            ..
        }
    ));

    server.reset().await;
    mount_repository(&server).await;
    mount_open_issues(
        &server,
        vec![
            issue_with_comment_count(91, 11, "first", "2026-09-20T09:30:00Z", 1),
            issue_with_comment_count(92, 12, "second", "2026-09-21T10:00:00Z", 2),
        ],
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/11/comments"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/12/comments"))
        .and(query_param("page", "2"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!([comment(1203, "second replacement")])),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/12/comments"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Link", "<?page=2>; rel=\"next\"")
                .set_body_json(json!([comment(1202, "first replacement")])),
        )
        .expect(1)
        .mount(&server)
        .await;
    let retried =
        sync_once_with_comments(&archive, &server, selector, SyncThreadScope::Open, true).await;
    assert_eq!(retried.outcome, OperationOutcome::Complete);
    assert_eq!(comment_bodies(&archive, 12).await.len(), 2);
    let resolved_failure = archive
        .run_detail(partial.run.id)
        .await
        .expect("load earlier run")
        .expect("earlier run")
        .failures
        .remove(0);
    assert_eq!(resolved_failure.retry_count, 1);
    assert!(resolved_failure.resolved_at.is_some());
    assert_eq!(resolved_failure.retry_run_id, Some(retried.run.id));

    archive.close().await;
    remove_archive(&archive_path);
}

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
    let initial = sync_once_with_comments(
        &archive,
        &server,
        selector.clone(),
        SyncThreadScope::Open,
        true,
    )
    .await;
    assert_eq!(initial.outcome, OperationOutcome::Complete);

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
    let partial =
        sync_once_with_comments(&archive, &server, selector, SyncThreadScope::Open, true).await;
    assert!(matches!(partial.outcome, OperationOutcome::Partial { .. }));
    assert!(comment_bodies(&archive, 11).await.is_empty());
    assert_eq!(comment_bodies(&archive, 12).await, ["preserved"]);
    assert!(matches!(
        comment_coverage(&thread_summary(&archive, 11).await).state(),
        CoverageState::Complete { item_count: 0, .. }
    ));
    assert!(matches!(
        comment_coverage(&thread_summary(&archive, 12).await).state(),
        CoverageState::Incomplete {
            received_items: 0,
            ..
        }
    ));

    archive.close().await;
    remove_archive(&archive_path);
}

#[tokio::test]
async fn comment_failure_ledger_error_retains_the_provider_failure() {
    let server = MockServer::start().await;
    mount_repository(&server).await;
    mount_open_issues(
        &server,
        vec![issue_with_comment_count(
            91,
            11,
            "first",
            "2026-09-20T09:30:00Z",
            1,
        )],
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues/11/comments"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let archive_path = temporary_archive_path();
    let archive = Archive::create(&archive_path)
        .await
        .expect("create archive");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&archive_path)
                .foreign_keys(true),
        )
        .await
        .expect("open archive for ledger trigger");
    sqlx::query(
        "CREATE TRIGGER reject_comment_failure BEFORE INSERT ON failures WHEN NEW.family = 'comments' BEGIN SELECT RAISE(ABORT, 'comment ledger unavailable'); END",
    )
    .execute(&pool)
    .await
    .expect("install failure trigger");
    pool.close().await;

    let selector = "owner/repo"
        .parse::<RepositorySelector>()
        .expect("selector");
    let clients = clients_for(&server, &selector);
    let error = sync_repositories(
        &archive,
        &clients,
        &SyncRequest {
            repositories: vec![selector],
            all: false,
            scope: SyncThreadScope::Open,
            include_comments: true,
            include_reviews: false,
            include_review_threads: false,
            parent_run: None,
        },
        &CancellationToken::new(),
        None,
    )
    .await
    .expect_err("failure ledger trigger should abort the report");
    match error {
        EngineError::FailureLedger { original, source } => {
            assert_eq!(original.kind, FailureKind::ProviderResponse);
            assert!(original.message.contains("HTTP 404"));
            assert!(source.to_string().contains("comment ledger unavailable"));
            assert_eq!(
                EngineError::FailureLedger { original, source }.code(),
                "failure_ledger_write_failed"
            );
        }
        other => panic!("expected preserved provider failure, got {other}"),
    }

    archive.close().await;
    remove_archive(&archive_path);
}

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
        .child_family_members::<forgesync_core::PullRequestMetadata>(
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

async fn sync_once(
    archive: &Archive,
    server: &MockServer,
    selector: RepositorySelector,
    scope: SyncThreadScope,
) -> forgesync_engine::SyncReport {
    sync_once_with_comments(archive, server, selector, scope, false).await
}

async fn sync_once_with_comments(
    archive: &Archive,
    server: &MockServer,
    selector: RepositorySelector,
    scope: SyncThreadScope,
    include_comments: bool,
) -> forgesync_engine::SyncReport {
    sync_once_with_families(
        archive,
        server,
        selector,
        scope,
        include_comments,
        false,
        false,
    )
    .await
}

async fn sync_once_with_families(
    archive: &Archive,
    server: &MockServer,
    selector: RepositorySelector,
    scope: SyncThreadScope,
    include_comments: bool,
    include_reviews: bool,
    include_review_threads: bool,
) -> forgesync_engine::SyncReport {
    let clients = clients_for(server, &selector);
    sync_repositories(
        archive,
        &clients,
        &SyncRequest {
            repositories: vec![selector],
            all: false,
            scope,
            include_comments,
            include_reviews,
            include_review_threads,
            parent_run: None,
        },
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report")
}

fn clients_for(
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

async fn mount_open_issues(server: &MockServer, issues: Vec<serde_json::Value>) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("state", "open"))
        .respond_with(ResponseTemplate::new(200).set_body_json(issues))
        .mount(server)
        .await;
}

fn pull_request_issue(updated_at: &str) -> serde_json::Value {
    let mut issue = issue(1802, 18, "selected change");
    issue["updated_at"] = json!(updated_at);
    issue["comments"] = json!(1);
    issue["html_url"] = json!("https://github.com/owner/repo/pull/18");
    issue["pull_request"] = json!({
        "url": "https://api.github.com/repos/owner/repo/pulls/18"
    });
    issue
}

async fn mount_pull_request_metadata(server: &MockServer, head_sha: &str, merged: bool) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/18"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "base": {
                "ref": "main",
                "sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "repo": { "id": 41, "full_name": "owner/repo" }
            },
            "head": {
                "ref": "topic",
                "sha": head_sha,
                "repo": { "id": 41, "full_name": "owner/repo" }
            },
            "draft": false,
            "merged": merged
        })))
        .mount(server)
        .await;
}

async fn mount_pull_reviews(server: &MockServer, status: u16, reviews: Vec<serde_json::Value>) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/pulls/18/reviews"))
        .and(query_param("per_page", "100"))
        .respond_with(ResponseTemplate::new(status).set_body_json(reviews))
        .mount(server)
        .await;
}

async fn mount_graphql_review_threads(server: &MockServer, response: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("reviewThreads(first: 100"))
        .and(body_string_contains("\"cursor\":null"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .mount(server)
        .await;
}

fn review_thread(id: &str, is_resolved: bool) -> serde_json::Value {
    json!({
        "id": id,
        "isResolved": is_resolved,
        "isOutdated": false,
        "path": "src/lib.rs",
        "line": 42,
        "startLine": null,
        "viewerCanResolve": true,
        "comments": {
            "nodes": [],
            "pageInfo": {"hasNextPage": false, "endCursor": null}
        }
    })
}

fn review_thread_page(
    review_threads: Vec<serde_json::Value>,
    has_next_page: bool,
    end_cursor: Option<&str>,
) -> serde_json::Value {
    json!({
        "data": {"repository": {"pullRequest": {"reviewThreads": {
            "nodes": review_threads,
            "pageInfo": {"hasNextPage": has_next_page, "endCursor": end_cursor}
        }}}}
    })
}

fn pull_review(id: u64, commit_sha: &str) -> serde_json::Value {
    json!({
        "id": id,
        "state": "CHANGES_REQUESTED",
        "body": "Please revise this change.",
        "submitted_at": "2026-09-19T12:00:00Z",
        "commit_id": commit_sha,
        "user": { "id": 51, "login": "reviewer", "type": "User" },
        "author_association": "MEMBER"
    })
}

async fn mount_comments(server: &MockServer, number: u64, comments: Vec<serde_json::Value>) {
    Mock::given(method("GET"))
        .and(path(format!(
            "/api/v3/repos/owner/repo/issues/{number}/comments"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(comments))
        .mount(server)
        .await;
}

async fn thread_summary(archive: &Archive, number: u64) -> forgesync_store::ThreadSummary {
    archive
        .query_threads(&ThreadQuery {
            repositories: Vec::new(),
            kind: None,
            state: ThreadStateFilter::All,
            match_expression: None,
            updated_since: None,
            sort: ThreadSort::Updated,
            limit: NonZeroU32::new(1000).expect("positive limit"),
            offset: 0,
        })
        .await
        .expect("query all threads")
        .items
        .into_iter()
        .find(|thread| thread.discussion.id.number().get() == number)
        .expect("thread summary")
}

fn comment_coverage(summary: &forgesync_store::ThreadSummary) -> &forgesync_core::Coverage {
    summary
        .coverage
        .iter()
        .find(|coverage| coverage.family() == EvidenceFamily::Comments)
        .expect("comment coverage")
}

fn review_coverage(summary: &forgesync_store::ThreadSummary) -> &forgesync_core::Coverage {
    summary
        .coverage
        .iter()
        .find(|coverage| coverage.family() == EvidenceFamily::Reviews)
        .expect("review coverage")
}

fn review_thread_coverage(summary: &forgesync_store::ThreadSummary) -> &forgesync_core::Coverage {
    summary
        .coverage
        .iter()
        .find(|coverage| coverage.family() == EvidenceFamily::ReviewThreads)
        .expect("review-thread coverage")
}

async fn comment_bodies(archive: &Archive, number: u64) -> Vec<String> {
    let summary = thread_summary(archive, number).await;
    archive
        .child_family_members::<Comment>(&summary.discussion.id, EvidenceFamily::Comments)
        .await
        .expect("read canonical comments")
        .into_iter()
        .map(|item| item.payload.body)
        .collect()
}

async fn review_members(
    archive: &Archive,
    number: u64,
) -> Vec<forgesync_store::StagedItem<Review>> {
    let summary = thread_summary(archive, number).await;
    archive
        .child_family_members::<Review>(&summary.discussion.id, EvidenceFamily::Reviews)
        .await
        .expect("read canonical reviews")
}

async fn review_thread_members(
    archive: &Archive,
    number: u64,
) -> Vec<forgesync_store::StagedItem<ReviewThread>> {
    let summary = thread_summary(archive, number).await;
    archive
        .child_family_members::<ReviewThread>(&summary.discussion.id, EvidenceFamily::ReviewThreads)
        .await
        .expect("read canonical review threads")
}

async fn mount_repository(server: &MockServer) {
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

fn issue(id: u64, number: u64, title: &str) -> serde_json::Value {
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

fn issue_with_comment_count(
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

fn comment(id: u64, body: &str) -> serde_json::Value {
    json!({
        "id": id,
        "body": body,
        "created_at": "2026-09-19T08:00:00Z",
        "updated_at": "2026-09-19T08:00:00Z",
        "user": { "login": "reviewer" }
    })
}

async fn thread_count(archive: &Archive) -> usize {
    archive
        .query_threads(&ThreadQuery {
            repositories: Vec::new(),
            kind: None,
            state: ThreadStateFilter::All,
            match_expression: None,
            updated_since: None,
            sort: ThreadSort::Updated,
            limit: NonZeroU32::new(20).expect("positive limit"),
            offset: 0,
        })
        .await
        .expect("query threads")
        .items
        .len()
}

fn temporary_archive_path() -> PathBuf {
    let next = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-sync-workflow-{}-{next}.sqlite",
        std::process::id()
    ))
}

fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
}
