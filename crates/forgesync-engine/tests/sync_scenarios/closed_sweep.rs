//! Failed closed sweeps retain their source watermark.

use forgesync_core::identity::GitHubHost;
use forgesync_core::outcome::OperationOutcome;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::sync::{SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_store::archive::Archive;
use serde_json::json;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fixture_archive::{remove_archive, temporary_archive_path};
use super::fixture_issues::{clients_for, mount_repository};

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
    let clients = clients_for(&server, &selector);
    let request = SyncRequest {
        repositories: vec![selector.clone()],
        all: false,
        scope: SyncThreadScope::Closed,
        include_comments: false,
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
    let host = GitHubHost::parse("github.com").expect("fixture host");
    let repository = archive
        .find_repository(&host, "owner", "repo")
        .await
        .expect("find repository");
    let repository = repository.expect("registered repository");
    let original_watermark = archive
        .closed_sweep_watermark(&repository.id)
        .await
        .expect("read successful watermark");
    let original_watermark = original_watermark.expect("initial watermark exists");

    // Move the checkpoint back to model an archive that was offline for a long interval.
    let historical_watermark = UtcTimestamp::parse("2025-01-01T00:00:00Z").expect("timestamp");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&archive_path)
                .create_if_missing(false)
                .foreign_keys(true),
        )
        .await
        .expect("open archive for test checkpoint adjustment");
    let adjustment = sqlx::query(
        "UPDATE repository_checkpoints SET watermark_us = ? WHERE repository_id = (SELECT id FROM repositories WHERE host = 'github.com' AND provider_id = '41') AND checkpoint = 'closed_sweep'",
    )
    .bind(historical_watermark.unix_microseconds())
    .execute(&pool)
    .await
    .expect("move closed watermark into the past");
    assert_eq!(adjustment.rows_affected(), 1);
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
    let clients = clients_for(&server, &selector);
    let request = SyncRequest {
        repositories: vec![selector.clone()],
        all: false,
        scope: SyncThreadScope::Closed,
        include_comments: false,
        include_reviews: false,
        include_review_threads: false,
        parent_run: None,
    };
    let failed = sync_repositories(
        &archive,
        &clients,
        &request,
        &CancellationToken::new(),
        None,
    )
    .await
    .expect("durable sync report");
    assert!(matches!(failed.outcome, OperationOutcome::Failed { .. }));
    let unchanged_watermark = archive
        .closed_sweep_watermark(&repository.id)
        .await
        .expect("read unchanged watermark");
    assert_eq!(unchanged_watermark, Some(historical_watermark));

    server.reset().await;
    mount_repository(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/owner/repo/issues"))
        .and(query_param("state", "closed"))
        .and(query_param("since", since))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let clients = clients_for(&server, &selector);
    let request = SyncRequest {
        repositories: vec![selector],
        all: false,
        scope: SyncThreadScope::Closed,
        include_comments: false,
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
    let advanced_watermark = archive
        .closed_sweep_watermark(&repository.id)
        .await
        .expect("read advanced watermark");
    let advanced_watermark = advanced_watermark.expect("advanced watermark exists");
    assert!(advanced_watermark > historical_watermark);
    assert!(advanced_watermark > original_watermark);

    archive.close().await;
    remove_archive(&archive_path);
}
