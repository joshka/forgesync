use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use forgesync_core::UtcTimestamp;
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

    let doctor = archive.doctor().await.expect("read archive diagnostics");
    assert!(doctor.healthy, "doctor report: {doctor:?}");
    assert_eq!(doctor.checks.len(), 4);
    assert!(doctor.checks.iter().all(|check| check.healthy));
    assert!(doctor.diagnostics.schema.history_valid);
    assert_eq!(
        doctor.diagnostics.schema.current_version,
        doctor.diagnostics.schema.supported_version
    );
    assert!(!doctor.diagnostics.lease.held);
    assert_eq!(doctor.diagnostics.work.unresolved_failures, 0);
    let before_status = archive.archive_status().await.expect("read status");
    assert_eq!(before_status.diagnostics.work.failed_jobs, 0);
    assert_eq!(before_status.diagnostics.work.deferred_jobs, 0);
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
async fn diagnostics_report_schema_lease_and_unresolved_family_work_read_only() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    archive.close().await;

    let pool = writable_pool(&path).await;
    let repository_payload = serde_json::json!({
        "id": { "host": "github.com", "provider_id": "41" },
        "owner": "owner",
        "name": "repo",
        "full_name": "owner/repo",
        "default_branch": null,
        "updated_at": null,
        "provider_data": {}
    });
    let repository_id: i64 = sqlx::query_scalar(
        "INSERT INTO repositories (host, provider_id, owner, name, full_name, provider_data_json, payload_json) VALUES ('github.com', '41', 'owner', 'repo', 'owner/repo', '{}', ?) RETURNING id",
    )
    .bind(repository_payload.to_string())
    .fetch_one(&pool)
    .await
    .expect("insert repository fixture");
    let run_id: i64 = sqlx::query_scalar(
        "INSERT INTO runs (status, started_at_us, updated_at_us, scope_json) VALUES ('partial', 1000, 1000, '{}') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert run fixture");
    for (family, status) in [("comments", "failed"), ("reviews", "deferred")] {
        let job_id: i64 = sqlx::query_scalar(
            "INSERT INTO jobs (run_id, repository_id, family, scope_key, status, started_at_us, updated_at_us) VALUES (?, ?, ?, 'open', ?, 1000, 1000) RETURNING id",
        )
        .bind(run_id)
        .bind(repository_id)
        .bind(family)
        .bind(status)
        .fetch_one(&pool)
        .await
        .expect("insert job fixture");
        sqlx::query(
            "INSERT INTO failures (run_id, job_id, repository_id, family, target_key, scope_key, failure_json, created_at_us) VALUES (?, ?, ?, ?, 'owner/repo', 'open', '{\"kind\":\"network\",\"message\":\"fixture failure\"}', 1000)",
        )
        .bind(run_id)
        .bind(job_id)
        .bind(repository_id)
        .bind(family)
        .execute(&pool)
        .await
        .expect("insert failure fixture");
    }
    pool.close().await;

    let archive = Archive::open_read_write(&path)
        .await
        .expect("open archive for lease");
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock after Unix epoch");
    let now = UtcTimestamp::from_unix_microseconds(
        i64::try_from(elapsed.as_micros()).expect("timestamp fits archive"),
    )
    .expect("valid timestamp");
    let lease = archive
        .acquire_archive_lease(now, std::time::Duration::from_secs(60))
        .await
        .expect("acquire archive lease");
    let readonly = Archive::open_read_only(&path)
        .await
        .expect("open read-only archive while leased");
    let diagnostics = readonly.diagnostics().await.expect("read diagnostics");
    assert!(diagnostics.schema.history_valid);
    assert!(diagnostics.lease.held);
    assert!(diagnostics.lease.owner_id.is_some());
    assert_eq!(diagnostics.work.failed_jobs, 1);
    assert_eq!(diagnostics.work.deferred_jobs, 1);
    assert_eq!(diagnostics.work.unresolved_failures, 2);
    assert_eq!(diagnostics.work.failures_by_family[1].unresolved, 1);
    assert_eq!(diagnostics.work.failures_by_family[3].unresolved, 1);
    assert!(readonly.doctor().await.expect("doctor").healthy);
    readonly.close().await;
    archive
        .release_archive_lease(&lease, now)
        .await
        .expect("release archive lease");
    archive.close().await;
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
