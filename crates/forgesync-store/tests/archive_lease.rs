//! # Archive lease integration
//!
//! These cases exercise lease ownership and guarded writes using an on-disk SQLite archive. They
//! protect coordination across independent operations and rejection of stale ownership. A lease
//! bounds workflow access; individual writes still use transactions.
//!
//! Independent archive handles establish contention against the same persisted lease row.
//! Takeover cases first claim a historical expired lease, then acquire a real current owner.
//! Guarded reservation must reject the stale token while preserving the first sequence for its
//! successor. Stale release separately checks that an old owner cannot clear that successor.
//! Clock sampling is necessary because guarded writes check expiry against process time.
//! Fixtures allocate paths only; each scenario shows its acquisition and archive operations.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;

/// Separates archive filenames for concurrent tests within this process.
static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

#[tokio::test]
async fn a_second_archive_handle_cannot_acquire_an_active_lease() {
    let path = temporary_archive_path();
    let first = Archive::create(&path).await.expect("create archive");
    let second = Archive::open_read_write(&path)
        .await
        .expect("open second writer");
    let now = now_utc();
    let _lease = first
        .acquire_archive_lease(now, Duration::from_secs(60))
        .await
        .expect("acquire first lease");

    assert!(matches!(
        second
            .acquire_archive_lease(now, Duration::from_secs(60))
            .await,
        Err(StoreError::ArchiveLeaseHeld)
    ));

    first.close().await;
    second.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn a_stale_fencing_token_cannot_write_after_a_new_owner_takes_over() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let expired = UtcTimestamp::parse("2020-01-01T00:00:00Z").expect("timestamp");
    let stale_lease = archive
        .acquire_archive_lease(expired, Duration::from_secs(1))
        .await
        .expect("claim expired lease");
    let now = now_utc();
    let current_lease = archive
        .acquire_archive_lease(now, Duration::from_secs(60))
        .await
        .expect("claim lease with a new fence");

    assert!(matches!(
        archive
            .reserve_observation_sequence_fenced(now, &stale_lease)
            .await,
        Err(StoreError::ArchiveLeaseLost)
    ));
    let sequence = archive
        .reserve_observation_sequence_fenced(now, &current_lease)
        .await
        .expect("current owner can reserve");
    assert_eq!(sequence.get(), 1);

    archive.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn stale_release_cannot_clear_a_successors_lease() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let expired = UtcTimestamp::parse("2020-01-01T00:00:00Z").expect("timestamp");
    let stale = archive
        .acquire_archive_lease(expired, Duration::from_secs(1))
        .await
        .expect("claim expired lease");
    let now = now_utc();
    let current = archive
        .acquire_archive_lease(now, Duration::from_secs(60))
        .await
        .expect("claim successor lease");

    let released = archive
        .release_archive_lease(&stale, now)
        .await
        .expect("stale release");

    assert!(!released);
    let sequence = archive
        .reserve_observation_sequence_fenced(now, &current)
        .await
        .expect("successor remains authorized");
    assert_eq!(sequence.get(), 1);
    archive.close().await;
    remove_archive(&path);
}

/// Samples current time so newly acquired tokens remain eligible at the guarded-write boundary.
fn now_utc() -> UtcTimestamp {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after Unix epoch");
    let microseconds = i64::try_from(elapsed.as_micros()).expect("timestamp fits archive");
    UtcTimestamp::from_unix_microseconds(microseconds).expect("valid timestamp")
}

/// Allocates a unique path without creating or opening the archive used by a scenario.
fn temporary_archive_path() -> PathBuf {
    let next = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-archive-lease-{}-{next}.sqlite",
        std::process::id()
    ))
}

/// Removes the closed database and possible WAL sidecars on a best-effort basis.
fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
}
