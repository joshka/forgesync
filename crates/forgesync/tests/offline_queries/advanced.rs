//! # Offline advanced scenarios
//!
//! Explicit advanced FTS interpretation accepts valid syntax and reports malformed syntax.
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
async fn advanced_fts_search_accepts_explicit_syntax() {
    let path = temporary_archive_path();
    seed_archive(&path).await;

    let before = Archive::open_read_only(&path)
        .await
        .expect("open archive before local queries");
    let status_before = before.archive_status().await.expect("read status before");
    before.close().await;

    let advanced_search = forgesync()
        .args([
            "search",
            "issues OR cache",
            "--mode",
            "advanced-fts",
            "--archive",
        ])
        .arg(&path)
        .arg("--json")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .output()
        .expect("run advanced search offline");
    assert!(advanced_search.status.success());
    let advanced_json: serde_json::Value =
        serde_json::from_slice(&advanced_search.stdout).expect("advanced search JSON");
    assert_eq!(advanced_json["data"]["items"].as_array().unwrap().len(), 1);

    let after = Archive::open_read_only(&path)
        .await
        .expect("open archive after local queries");
    let status_after = after.archive_status().await.expect("read status after");
    assert_eq!(status_after, status_before);
    after.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn malformed_fts_returns_typed_query_error() {
    let path = temporary_archive_path();
    seed_archive(&path).await;

    let before = Archive::open_read_only(&path)
        .await
        .expect("open archive before local queries");
    let status_before = before.archive_status().await.expect("read status before");
    before.close().await;

    let malformed_advanced = forgesync()
        .args(["search", "NEAR(", "--mode", "advanced-fts", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run malformed advanced search");
    assert_eq!(malformed_advanced.status.code(), Some(1));
    let malformed_json: serde_json::Value =
        serde_json::from_slice(&malformed_advanced.stdout).expect("query error JSON");
    assert_eq!(malformed_json["error"]["code"], "search_query_invalid");

    let after = Archive::open_read_only(&path)
        .await
        .expect("open archive after local queries");
    let status_after = after.archive_status().await.expect("read status after");
    assert_eq!(status_after, status_before);
    after.close().await;
    remove_archive(&path);
}
