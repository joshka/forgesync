//! # Status command contract
//!
//! This case asks status to inspect a deliberately missing path. The command must return its
//! versioned failure envelope without creating an archive or attempting migration/acquisition.
//! A unique path makes absence independent of other process scenarios.
//!
//! Assertions identify the command, schema version, typed error, and failure exit status.
//! No success data is returned and the requested database remains absent. Existing-archive status
//! and health output are covered by lifecycle and presentation scenarios; store tests cover the
//! diagnostic read mechanics. This file owns the missing-path process boundary only.

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
    assert_eq!(result["command"], "archive status");
    assert_eq!(result["error"]["code"], "archive_missing");
    assert!(result.get("data").is_none());
}
