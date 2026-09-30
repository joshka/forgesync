//! # Offline validation scenarios
//!
//! Invalid selectors and page limits are rejected by the process argument boundary.
//! Each scenario builds an independent on-disk archive with one known discussion.
//! Process arguments and expected output are visible beside the operation under test.
//! No helper executes a query or chooses a scenario on behalf of these cases.
//!
//! Cases compare archive status before and after their selected command, including rejection.
//! That comparison covers the reported durable state, not every possible SQLite byte change.
//! Separate presentation cases cover human section ordering rather than query interpretation.
//! Construction and cleanup live in `fixture`; store and engine suites own query mechanics.

use forgesync_store::archive::Archive;

use crate::fixture::{forgesync, remove_archive, seed_archive, temporary_archive_path};

#[tokio::test]
async fn invalid_thread_reference_returns_usage_status() {
    let path = temporary_archive_path();
    seed_archive(&path).await;

    let before = Archive::open_read_only(&path)
        .await
        .expect("open archive before local queries");
    let status_before = before.archive_status().await.expect("read status before");
    before.close().await;

    let invalid_reference = forgesync()
        .args(["thread", "show", "17", "--archive"])
        .arg(&path)
        .output()
        .expect("run invalid reference");
    assert_eq!(invalid_reference.status.code(), Some(2));

    let after = Archive::open_read_only(&path)
        .await
        .expect("open archive after local queries");
    let status_after = after.archive_status().await.expect("read status after");
    assert_eq!(status_after, status_before);
    after.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn zero_search_limit_returns_usage_status() {
    let path = temporary_archive_path();
    seed_archive(&path).await;

    let before = Archive::open_read_only(&path)
        .await
        .expect("open archive before local queries");
    let status_before = before.archive_status().await.expect("read status before");
    before.close().await;

    let invalid_limit = forgesync()
        .args(["search", "issues", "--limit", "0", "--archive"])
        .arg(&path)
        .output()
        .expect("run invalid limit");
    assert_eq!(invalid_limit.status.code(), Some(2));

    let after = Archive::open_read_only(&path)
        .await
        .expect("open archive after local queries");
    let status_after = after.archive_status().await.expect("read status after");
    assert_eq!(status_after, status_before);
    after.close().await;
    remove_archive(&path);
}
