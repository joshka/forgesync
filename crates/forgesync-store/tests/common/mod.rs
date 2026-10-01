//! Fixtures shared by the store integration suites.

#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forgesync_core::content::Repository;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;
use forgesync_store::leases::ArchiveLeaseToken;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

/// Returns a unique archive path without creating the file.
pub fn temporary_archive_path() -> PathBuf {
    std::env::temp_dir().join(format!("forgesync-store-{}.sqlite", uuid::Uuid::new_v4()))
}

/// Removes a closed archive and its SQLite sidecars, ignoring missing files.
pub fn remove_archive(path: &Path) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
}

/// Opens an existing database for raw fixture arrangement, bypassing archive validation.
pub async fn writable_pool(path: &Path) -> sqlx::SqlitePool {
    raw_pool(path, false).await
}

/// Opens an existing database for raw read-only inspection.
pub async fn read_only_pool(path: &Path) -> sqlx::SqlitePool {
    raw_pool(path, true).await
}

/// Opens a one-connection pool without creating or migrating the database.
async fn raw_pool(path: &Path, read_only: bool) -> sqlx::SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(false)
                .read_only(read_only)
                .foreign_keys(true),
        )
        .await
        .expect("open raw test pool")
}

/// Parses a fixed fixture timestamp.
pub fn timestamp(value: &str) -> UtcTimestamp {
    UtcTimestamp::parse(value).expect("valid timestamp")
}

/// Reads the process clock; fenced writes compare lease expiry against it.
pub fn now() -> UtcTimestamp {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch");
    UtcTimestamp::from_unix_microseconds(i64::try_from(elapsed.as_micros()).expect("clock range"))
        .expect("current timestamp")
}

/// Acquires a writer lease valid at the current process clock, as fenced writes require.
pub async fn lease(archive: &Archive) -> ArchiveLeaseToken {
    archive
        .acquire_archive_lease(now(), Duration::from_secs(600))
        .await
        .expect("acquire archive lease")
}

/// Builds GitHub repository metadata without registering it.
pub fn repository(owner: &str, name: &str, provider_id: &str) -> Repository {
    Repository {
        id: RepositoryId::new(
            GitHubHost::parse("github.com").expect("host"),
            ProviderId::new(provider_id).expect("repository provider ID"),
        ),
        owner: owner.to_owned(),
        name: name.to_owned(),
        full_name: format!("{owner}/{name}"),
        default_branch: Some("main".to_owned()),
        updated_at: Some(timestamp("2026-09-20T10:00:00Z")),
        provider_data: ProviderData::new(),
    }
}

/// Builds a discussion identity in `repository`.
pub fn thread_id(repository: &RepositoryId, provider_id: &str, number: u64) -> ThreadId {
    ThreadId::new(
        repository.clone(),
        ProviderId::new(provider_id).expect("thread provider ID"),
        ThreadNumber::new(number).expect("thread number"),
    )
}
