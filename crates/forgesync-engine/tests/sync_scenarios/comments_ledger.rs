//! Failure-ledger writes preserve the original provider failure.

use forgesync_core::coverage::FailureKind;
use forgesync_engine::error::EngineError;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::sync::{SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::fixture_archive::{remove_archive, temporary_archive_path};
use super::fixture_issues::{
    clients_for, issue_with_comment_count, mount_open_issues, mount_repository,
};

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
                .create_if_missing(false)
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
    assert_eq!(error.code(), "failure_ledger_write_failed");
    let source = std::error::Error::source(&error).expect("failure-ledger source");
    let store_error = source
        .downcast_ref::<StoreError>()
        .expect("typed store error in the source chain");
    assert!(matches!(store_error, StoreError::Database(_)));
    assert!(
        store_error
            .to_string()
            .contains("comment ledger unavailable")
    );
    let EngineError::FailureLedger { original, .. } = error else {
        panic!("expected preserved provider failure");
    };
    assert_eq!(original.kind, FailureKind::ProviderResponse);
    assert!(original.message.contains("HTTP 404"));

    archive.close().await;
    remove_archive(&archive_path);
}
