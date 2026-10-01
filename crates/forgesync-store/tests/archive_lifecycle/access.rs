//! # Archive identity and access modes
//!
//! These cases reopen one created archive in both access modes and reject an absent path.
//! Identity and schema version survive reopening; opening must not create a missing database.
//!
//! Each case invokes archive operations directly and closes handles before cleanup.
//! Shared infrastructure allocates filenames and opens raw pools; it executes no scenario.
//! SQLite is on disk so file effects, migration history, and pool behavior are observable.
//! SQL mutation is fixture arrangement rather than a supported application write path.
//! Provider acquisition, source membership, and engine retry are outside this lifecycle suite.
//! Failure assertions name the store variant or retained diagnostic facts under examination.

use std::time::Duration;

use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;

use crate::fixture::{remove_archive, temporary_archive_path};

#[tokio::test]
async fn reopening_preserves_archive_identity_and_access_mode() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let archive_id = archive.info().archive_id.clone();
    let schema_version = archive.info().schema_version;
    assert_eq!(archive.info().format_id, "forgesync");

    assert!(matches!(
        Archive::create(&path).await,
        Err(StoreError::AlreadyExists(_))
    ));
    archive.close().await;

    let archive = Archive::open_read_only(&path)
        .await
        .expect("open archive read-only");
    assert!(matches!(
        archive
            .acquire_archive_lease(archive.info().created_at, Duration::from_secs(1))
            .await,
        Err(StoreError::ReadOnlyArchive)
    ));
    assert_eq!(archive.info().archive_id, archive_id);
    assert_eq!(archive.info().schema_version, schema_version);

    archive.close().await;

    let archive = Archive::open_read_write(&path)
        .await
        .expect("open archive read-write");
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
    assert!(!path.exists());
}
