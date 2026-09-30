//! # Migration declarations and rejected history
//!
//! Current schema migration applies no changes, and absent paths are never created.
//! Raw SQL arranges newer, incomplete, and mismatched history for real opening to reject.
//!
//! Each case invokes archive operations directly and closes handles before cleanup.
//! Shared infrastructure allocates filenames and opens raw pools; it executes no scenario.
//! SQLite is on disk so file effects, migration history, and pool behavior are observable.
//! SQL mutation is fixture arrangement rather than a supported application write path.
//! Provider acquisition, source membership, and engine retry are outside this lifecycle suite.
//! Failure assertions name the store variant or retained diagnostic facts under examination.

use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;

use crate::fixture::{read_only_pool, remove_archive, temporary_archive_path, writable_pool};

#[tokio::test]
async fn current_schema_migration_applies_no_changes() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let schema_version = archive.info().schema_version;
    archive.close().await;

    let migration = Archive::migrate(&path).await.expect("migrate archive");
    assert_eq!(migration.previous_schema_version, schema_version);
    assert_eq!(migration.schema_version, schema_version);
    assert!(migration.applied_migrations.is_empty());

    remove_archive(&path);
}

#[tokio::test]
async fn migrating_a_missing_archive_does_not_create_a_file() {
    let path = temporary_archive_path();

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
