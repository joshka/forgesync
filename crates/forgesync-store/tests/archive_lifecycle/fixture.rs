//! # Archive lifecycle test infrastructure
//!
//! Filename allocation separates concurrently executing cases without creating an archive.
//! Raw pools open existing databases with foreign keys enabled and one connection.
//! Read-only pools inspect state; writable pools deliberately arrange invalid or diagnostic facts.
//! Neither pool creates or migrates a database, and neither performs scenario assertions.
//!
//! Tests own archive creation, operations, pool closure, and all expected outcomes.
//! Cleanup is best effort after handles close and visits a fixed SQLite sidecar list.
//! The sidecar loop releases resources rather than selecting test scenarios.
//! No provider request, application configuration, or credential lookup occurs here.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

/// Allocates distinct database filenames for concurrent cases within this process.
static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

/// Opens an existing database for raw inspection without creation or migration.
pub async fn read_only_pool(path: &PathBuf) -> sqlx::SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(false)
                .read_only(true)
                .foreign_keys(true),
        )
        .await
        .expect("open read-only inspection pool")
}

/// Opens an existing database for explicit fixture corruption, bypassing archive validation.
///
/// Only tests use this pool to arrange invalid ledger states; creation remains disabled.
pub async fn writable_pool(path: &PathBuf) -> sqlx::SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(false)
                .foreign_keys(true),
        )
        .await
        .expect("open writable test pool")
}

/// Allocates a process-local unique filename without creating or opening an archive.
pub fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-store-{}-{sequence}.sqlite",
        std::process::id()
    ))
}

/// Removes the closed database and its possible WAL sidecars on a best-effort basis.
///
/// The fixed suffix loop is cleanup only; it does not select scenarios or compute expectations.
pub fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
}
