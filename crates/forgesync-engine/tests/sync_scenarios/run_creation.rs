//! Failed run creation releases its writer fence.

use std::collections::HashMap;
use std::time::Duration;

use forgesync_core::identity::RunId;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_engine::error::EngineError;
use forgesync_engine::sync::{SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use tokio_util::sync::CancellationToken;

use super::fixture_archive::{remove_archive, temporary_archive_path};

#[tokio::test]
async fn failed_run_creation_releases_the_writer_fence() {
    let archive_path = temporary_archive_path();
    let archive = Archive::create(&archive_path)
        .await
        .expect("create archive");
    let request = SyncRequest {
        repositories: Vec::new(),
        all: true,
        scope: SyncThreadScope::All,
        include_comments: false,
        include_reviews: false,
        include_review_threads: false,
        parent_run: Some(RunId::new(999).expect("nonzero parent")),
    };
    let clients = HashMap::new();
    let cancellation = CancellationToken::new();

    let error = sync_repositories(&archive, &clients, &request, &cancellation, None)
        .await
        .expect_err("missing parent rejects run creation");

    let EngineError::Store(StoreError::Database(sqlx::Error::Database(database))) = &error else {
        panic!("missing parent must fail the run foreign-key constraint: {error}");
    };
    assert!(database.is_foreign_key_violation());
    assert!(!cancellation.is_cancelled());
    let runs = archive.list_runs(10).await.expect("read run list");
    assert!(runs.is_empty());
    // An old timestamp cannot expire a leaked current-time fence, so reacquisition proves release.
    let at = UtcTimestamp::parse("1970-01-01T00:00:00Z").expect("lease timestamp");
    let lease = archive
        .acquire_archive_lease(at, Duration::from_secs(60))
        .await
        .expect("failed run released its fence");
    let released = archive
        .release_archive_lease(&lease, at)
        .await
        .expect("release test fence");
    assert!(released);
    archive.close().await;
    remove_archive(&archive_path);
}
