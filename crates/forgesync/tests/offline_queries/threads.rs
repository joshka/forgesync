//! # Offline threads scenarios
//!
//! Local list filters and discussion selectors resolve retained archive content.
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
async fn thread_list_applies_local_scope_kind_and_state() {
    let path = temporary_archive_path();
    seed_archive(&path).await;

    let before = Archive::open_read_only(&path)
        .await
        .expect("open archive before local queries");
    let status_before = before.archive_status().await.expect("read status before");
    before.close().await;

    let thread_list = forgesync()
        .args(["--archive"])
        .arg(&path)
        .args([
            "thread",
            "list",
            "--repo",
            "example/project",
            "--kind",
            "issue",
            "--state",
            "open",
            "--json",
        ])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .output()
        .expect("run thread list offline");
    assert!(thread_list.status.success());
    let list_json: serde_json::Value =
        serde_json::from_slice(&thread_list.stdout).expect("thread list JSON");
    assert_eq!(list_json["command"], "thread list");
    assert_eq!(list_json["data"]["items"].as_array().unwrap().len(), 1);

    let after = Archive::open_read_only(&path)
        .await
        .expect("open archive after local queries");
    let status_after = after.archive_status().await.expect("read status after");
    assert_eq!(status_after, status_before);
    after.close().await;
    remove_archive(&path);
}

#[tokio::test]
async fn thread_show_resolves_local_identity() {
    let path = temporary_archive_path();
    seed_archive(&path).await;

    let before = Archive::open_read_only(&path)
        .await
        .expect("open archive before local queries");
    let status_before = before.archive_status().await.expect("read status before");
    before.close().await;

    let thread_show = forgesync()
        .args(["thread", "show", "example/project#17", "--archive"])
        .arg(&path)
        .arg("--json")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .output()
        .expect("run thread show offline");
    assert!(thread_show.status.success());
    let show_json: serde_json::Value =
        serde_json::from_slice(&thread_show.stdout).expect("thread show JSON");
    assert_eq!(show_json["command"], "thread show");
    assert_eq!(
        show_json["data"]["summary"]["thread"]["title"],
        "Issues OR cache timeout"
    );

    let after = Archive::open_read_only(&path)
        .await
        .expect("open archive after local queries");
    let status_after = after.archive_status().await.expect("read status after");
    assert_eq!(status_after, status_before);
    after.close().await;
    remove_archive(&path);
}
