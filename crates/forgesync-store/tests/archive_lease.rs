use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::{Archive, StoreError};

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
    let _current_lease = archive
        .acquire_archive_lease(now, Duration::from_secs(60))
        .await
        .expect("claim lease with a new fence");

    assert!(matches!(
        archive
            .reserve_observation_sequence_fenced(now, &stale_lease)
            .await,
        Err(StoreError::ArchiveLeaseLost)
    ));

    archive.close().await;
    remove_archive(&path);
}

fn now_utc() -> UtcTimestamp {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after Unix epoch");
    let microseconds = i64::try_from(elapsed.as_micros()).expect("timestamp fits archive");
    UtcTimestamp::from_unix_microseconds(microseconds).expect("valid timestamp")
}

fn temporary_archive_path() -> PathBuf {
    let next = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-archive-lease-{}-{next}.sqlite",
        std::process::id()
    ))
}

fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
}
