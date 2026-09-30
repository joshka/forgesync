//! # Offline keyword scenarios
//!
//! Keyword process results preserve literal query interpretation and empty-page success.
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
async fn ordinary_keyword_search_keeps_query_punctuation_literal() {
    let path = temporary_archive_path();
    seed_archive(&path).await;

    let before = Archive::open_read_only(&path)
        .await
        .expect("open archive before local queries");
    let status_before = before.archive_status().await.expect("read status before");
    before.close().await;

    let ordinary_search = forgesync()
        .args(["--archive"])
        .arg(&path)
        .args([
            "--json",
            "search",
            "issues OR (cache*)",
            "--repo",
            "example/project",
        ])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .output()
        .expect("run ordinary search offline");
    assert!(
        ordinary_search.status.success(),
        "{}",
        String::from_utf8_lossy(&ordinary_search.stderr)
    );
    let ordinary_json: serde_json::Value =
        serde_json::from_slice(&ordinary_search.stdout).expect("ordinary search JSON");
    assert_eq!(ordinary_json["command"], "search");
    assert_eq!(ordinary_json["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        ordinary_json["data"]["items"][0]["thread"]["title"],
        "Issues OR cache timeout"
    );
    assert!(ordinary_json["data"]["coverage"].is_array());

    let after = Archive::open_read_only(&path)
        .await
        .expect("open archive after local queries");
    let status_after = after.archive_status().await.expect("read status after");
    assert_eq!(status_after, status_before);
    after.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn missing_keyword_returns_successful_empty_page() {
    let path = temporary_archive_path();
    seed_archive(&path).await;

    let before = Archive::open_read_only(&path)
        .await
        .expect("open archive before local queries");
    let status_before = before.archive_status().await.expect("read status before");
    before.close().await;

    let empty_search = forgesync()
        .args(["search", "missing-term", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run empty search");
    assert_eq!(empty_search.status.code(), Some(0));
    let empty_json: serde_json::Value =
        serde_json::from_slice(&empty_search.stdout).expect("empty search JSON");
    assert_eq!(empty_json["data"]["items"].as_array().unwrap().len(), 0);
    assert!(empty_json["data"]["coverage"].is_array());

    let after = Archive::open_read_only(&path)
        .await
        .expect("open archive after local queries");
    let status_after = after.archive_status().await.expect("read status after");
    assert_eq!(status_after, status_before);
    after.close().await;
    remove_archive(&path);
}
