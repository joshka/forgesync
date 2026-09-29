//! # Status command contract
//!
//! These cases check what an operator sees when inspecting an existing archive. Status should
//! describe schema, work, and coverage without triggering acquisition or migration. The assertions
//! belong at the process boundary because wording and JSON shape are CLI-owned.

use super::{forgesync, temporary_archive_path};

#[test]
fn status_reports_missing_archive_as_json_without_creating_it() {
    let path = temporary_archive_path();
    let output = forgesync()
        .args(["archive", "status", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run status");

    assert_eq!(output.status.code(), Some(1));
    assert!(!path.exists());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).expect("error JSON");
    assert_eq!(result["schema_version"], 1);
    assert_eq!(result["error"]["code"], "archive_missing");
}
