use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use forgesync_core::{
    Comment, CoverageState, EvidenceFamily, FailureKind, GitHubHost, OperationOutcome, UtcTimestamp,
};
use forgesync_engine::{
    EngineError, RepositorySelector, SyncRequest, SyncThreadScope, sync_repositories,
};
use forgesync_github::{GitHubClient, GitHubClientConfig};
use forgesync_store::{Archive, SyncJobStatus, ThreadQuery, ThreadSort, ThreadStateFilter};
use serde_json::json;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path, query_param};
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
    let clients = clients_for(server, &selector);
    sync_repositories(
        archive,
        &clients,
        &SyncRequest {
            repositories: vec![selector],
            all: false,
            scope,
            include_comments,
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
