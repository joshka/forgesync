use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use forgesync_store::{Archive, StoreError};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

#[tokio::test]
async fn create_open_status_migrate_and_doctor_use_explicit_lifecycle() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let archive_id = archive.info().archive_id.clone();
    let schema_version = archive.info().schema_version;
    assert_eq!(archive.info().format_id, "forgesync");
    assert!(!archive.is_read_only());

    assert!(matches!(
        Archive::create(&path).await,
        Err(StoreError::AlreadyExists(_))
    ));
    archive.close().await;

    let archive = Archive::open_read_only(&path)
        .await
        .expect("open archive read-only");
    assert!(archive.is_read_only());
    assert_eq!(archive.info().archive_id, archive_id);
    assert_eq!(archive.info().schema_version, schema_version);

    let doctor = archive.doctor().await;
    assert!(doctor.healthy, "doctor report: {doctor:?}");
    assert_eq!(doctor.checks.len(), 3);
    assert!(doctor.checks.iter().all(|check| check.healthy));
    archive.close().await;

    let migration = Archive::migrate(&path).await.expect("migrate archive");
    assert_eq!(migration.previous_schema_version, schema_version);
    assert_eq!(migration.schema_version, schema_version);
    assert!(migration.applied_migrations.is_empty());

    let archive = Archive::open_read_write(&path)
        .await
        .expect("open archive read-write");
    assert!(!archive.is_read_only());
    archive.close().await;

    let pool = read_only_pool(&path).await;
    let persisted_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name LIKE '__forgesync_%'",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect tables");
    assert_eq!(persisted_tables, 0, "doctor probes must remain temporary");
    pool.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn opening_a_missing_archive_does_not_create_a_file() {
    let path = temporary_archive_path();

    assert!(matches!(
        Archive::open_read_only(&path).await,
        Err(StoreError::MissingArchive(_))
    ));
    assert!(matches!(
        Archive::migrate(&path).await,
        Err(StoreError::MissingArchive(_))
    ));
    assert!(!path.exists());
}

#[tokio::test]
async fn opening_a_newer_schema_fails_without_migrating_it() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let supported = archive.info().schema_version;
    archive.close().await;

    let pool = writable_pool(&path).await;
    let future_version = supported + 1;
    sqlx::query("UPDATE _sqlx_migrations SET version = ? WHERE version = ?")
        .bind(future_version)
        .bind(supported)
        .execute(&pool)
        .await
        .expect("make schema newer for the test");
    pool.close().await;

    assert!(matches!(
        Archive::open_read_only(&path).await,
        Err(StoreError::SchemaTooNew { found, .. }) if found == future_version
    ));

    let pool = read_only_pool(&path).await;
    let version: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .expect("read schema version");
    assert_eq!(
        version, future_version,
        "open must not change migration state"
    );
    pool.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn incomplete_migrations_are_rejected_without_changing_the_archive() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let schema_version = archive.info().schema_version;
    archive.close().await;

    let pool = writable_pool(&path).await;
    sqlx::query("UPDATE _sqlx_migrations SET success = 0 WHERE version = ?")
        .bind(schema_version)
        .execute(&pool)
        .await
        .expect("mark migration incomplete");
    pool.close().await;

    assert!(matches!(
        Archive::open_read_only(&path).await,
        Err(StoreError::MigrationHistoryDirty { version }) if version == schema_version
    ));

    let pool = read_only_pool(&path).await;
    let success: i64 = sqlx::query_scalar("SELECT success FROM _sqlx_migrations WHERE version = ?")
        .bind(schema_version)
        .fetch_one(&pool)
        .await
        .expect("read migration status");
    assert_eq!(success, 0, "open must not repair migration history");
    pool.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn migration_checksum_mismatch_is_rejected() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let schema_version = archive.info().schema_version;
    archive.close().await;

    let pool = writable_pool(&path).await;
    sqlx::query("UPDATE _sqlx_migrations SET checksum = X'00' WHERE version = ?")
        .bind(schema_version)
        .execute(&pool)
        .await
        .expect("change migration checksum");
    pool.close().await;

    assert!(matches!(
        Archive::open_read_only(&path).await,
        Err(StoreError::MigrationChecksumMismatch { version }) if version == schema_version
    ));

    let pool = read_only_pool(&path).await;
    let checksum: Vec<u8> =
        sqlx::query_scalar("SELECT checksum FROM _sqlx_migrations WHERE version = ?")
            .bind(schema_version)
            .fetch_one(&pool)
            .await
            .expect("read migration checksum");
    assert_eq!(checksum, [0], "open must not change migration history");
    pool.close().await;
    remove_archive(&path);
}

async fn read_only_pool(path: &PathBuf) -> sqlx::SqlitePool {
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

async fn writable_pool(path: &PathBuf) -> sqlx::SqlitePool {
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

fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-store-{}-{sequence}.sqlite",
        std::process::id()
    ))
}

fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
}
