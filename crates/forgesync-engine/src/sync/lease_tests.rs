//! # Lease renewal must leave the active workflow runnable
//!
//! The renewal fixture announces that it is waiting for a writer connection, then remains pending.
//! The operation can finish only after that boundary. This reproduces the scheduling dependency
//! of an archive transaction returning the sole writer connection without relying on SQL timing.
//! A bounded timeout catches a coordinator that awaits renewal while it stops polling the writer.
//! These tests exercise coordination; store suites separately cover real SQLite transactions.

use std::time::Duration;

use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::error::EngineError;
use crate::sync::lease::SyncLease;

#[tokio::test]
async fn acquisition_continues_while_renewal_waits_for_its_connection() {
    let directory =
        std::env::temp_dir().join(format!("forgesync-sync-lease-{}", std::process::id()));
    std::fs::create_dir(&directory).expect("create fixture directory");
    let archive = Archive::create(directory.join("archive.sqlite"))
        .await
        .expect("create archive");
    let caller = CancellationToken::new();
    let lease = SyncLease::acquire(&archive, &caller)
        .await
        .expect("acquire lease");
    let waiting = Notify::new();
    let operation = async {
        waiting.notified().await;
        Err(EngineError::InvalidSyncScope)
    };
    let renewal = async {
        waiting.notify_one();
        std::future::pending::<EngineError>().await
    };

    let result = tokio::time::timeout(Duration::from_secs(1), lease.maintain(operation, renewal))
        .await
        .expect("acquisition is polled while renewal waits");

    assert!(matches!(result, Err(EngineError::InvalidSyncScope)));
    assert!(!lease.cancellation.is_cancelled());
    assert!(!caller.is_cancelled());
    lease.abandon().await;
    archive.close().await;
    std::fs::remove_dir_all(directory).expect("remove fixture directory");
}

#[tokio::test]
async fn renewal_failure_drains_cleanup_without_cancelling_the_caller() {
    let directory =
        std::env::temp_dir().join(format!("forgesync-sync-renewal-{}", std::process::id()));
    std::fs::create_dir(&directory).expect("create fixture directory");
    let archive = Archive::create(directory.join("archive.sqlite"))
        .await
        .expect("create archive");
    let caller = CancellationToken::new();
    let lease = SyncLease::acquire(&archive, &caller)
        .await
        .expect("acquire lease");
    let mut cleaned_up = false;
    let operation = async {
        lease.cancellation.cancelled().await;
        cleaned_up = true;
        Err(EngineError::InvalidSyncScope)
    };

    let result = lease
        .maintain(operation, async { StoreError::ArchiveLeaseLost.into() })
        .await;

    assert!(matches!(
        result,
        Err(EngineError::Store(StoreError::ArchiveLeaseLost))
    ));
    assert!(cleaned_up);
    assert!(lease.cancellation.is_cancelled());
    assert!(!caller.is_cancelled());
    lease.abandon().await;
    archive.close().await;
    std::fs::remove_dir_all(directory).expect("remove fixture directory");
}
