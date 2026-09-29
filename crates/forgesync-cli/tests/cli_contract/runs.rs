//! # Run-history command contract
//!
//! These cases present durable run and retry information through CLI list and show commands. A run
//! records an attempt, not complete source coverage. Keep output expectations tied to the stored
//! ledger so partial failures remain visible after later work.

use super::{forgesync, remove_archive, temporary_archive_path};

#[test]
fn run_list_show_and_retry_use_durable_run_records() {
    let path = temporary_archive_path();
    let init = forgesync()
        .args(["archive", "init", "--archive"])
        .arg(&path)
        .output()
        .expect("initialize archive");
    assert!(init.status.success());

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

    let show = forgesync()
        .args(["run", "show", "1", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("show run");
    assert_eq!(show.status.code(), Some(1));
    let show_json: serde_json::Value = serde_json::from_slice(&show.stdout).expect("run show JSON");
    assert_eq!(show_json["error"]["code"], "run_missing");

    let retry = forgesync()
        .args(["run", "retry", "1", "--family", "comments", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("retry run");
    assert_eq!(retry.status.code(), Some(1));
    let retry_json: serde_json::Value =
        serde_json::from_slice(&retry.stdout).expect("run retry JSON");
    assert_eq!(retry_json["error"]["code"], "run_missing");
    remove_archive(&path);
}
