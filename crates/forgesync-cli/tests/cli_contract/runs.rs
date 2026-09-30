//! # Empty run-history process contract
//!
//! These cases distinguish a successful empty run list from missing-run show and retry failures.
//! Each archive is created without starting a sync run, so the expected absence is deliberate.
//! The selected command and diagnostic are visible in each named case.
//!
//! Show and retry share only parameterized command data; no helper executes their workflows.
//! Missing-run validation occurs before acquisition, so provider credentials are unnecessary.
//! The store/engine suites cover real run ledgers, partial progress, and retry planning.
//! Construction closes its writable handle before process execution; cleanup owns SQLite sidecars.

use forgesync_store::archive::Archive;

use crate::{forgesync, remove_archive, temporary_archive_path};

#[tokio::test]
async fn empty_run_list_returns_successful_empty_json() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path)
        .await
        .expect("create archive without runs");
    archive.close().await;

    let list = forgesync()
        .args(["run", "list", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("list runs");
    assert!(list.status.success());
    let list_json: serde_json::Value = serde_json::from_slice(&list.stdout).expect("run list JSON");
    assert_eq!(list_json["command"], "run list");
    assert_eq!(list_json["data"], serde_json::json!([]));

    remove_archive(&path);
}

#[rstest::rstest]
#[case::show(&["run", "show", "1"], "run show")]
#[case::retry(&["run", "retry", "1", "--family", "comments"], "run retry")]
#[tokio::test]
async fn missing_run_returns_typed_failure(#[case] arguments: &[&str], #[case] command: &str) {
    let path = temporary_archive_path();
    let archive = Archive::create(&path)
        .await
        .expect("create archive without runs");
    archive.close().await;

    let result = forgesync()
        .args(arguments)
        .arg("--archive")
        .arg(&path)
        .arg("--json")
        .output()
        .expect("inspect or retry absent run");
    let envelope: serde_json::Value =
        serde_json::from_slice(&result.stdout).expect("run error JSON");

    assert_eq!(result.status.code(), Some(1));
    assert_eq!(envelope["command"], command);
    assert_eq!(envelope["error"]["code"], "run_missing");
    assert!(envelope.get("data").is_none());
    remove_archive(&path);
}
