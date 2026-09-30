//! # Comment acquisition scenarios
//!
//! These cases exercise paginated comment collection through the engine and store. They focus on
//! completeness, failure scope, and canonical membership after interruption. A partial page set
//! should remain recorded as an attempt without replacing a prior complete family.

use forgesync_core::coverage::{CoverageState, FailureKind};
use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::error::EngineError;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::sync::{SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_store::archive::Archive;
use serde_json::json;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{
    clients_for, comment, comment_bodies, comment_coverage, issue_with_comment_count,
    mount_comments, mount_open_issues, mount_repository, remove_archive, temporary_archive_path,
    thread_summary,
};

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
    let clients = clients_for(&server, &selector);
    let request = SyncRequest {
        repositories: vec![selector.clone()],
        all: false,
        scope: SyncThreadScope::Open,
        include_comments: false,
        include_reviews: false,
        include_review_threads: false,
        parent_run: None,
    };
    let parent_refresh = sync_repositories(
        &archive,
        &clients,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report");
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
    let partial = sync_repositories(
        &archive,
        &clients,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report");
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
    let retried = sync_repositories(
        &archive,
        &clients,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report");
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
