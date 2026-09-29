//! # Archive command contract
//!
//! These cases exercise explicit archive creation and maintenance through the executable. They
//! check observable output and state so a command refactor cannot silently create, migrate, or
//! repair on a read path. The store unit and integration suites cover lower-level database
//! invariants.

use super::{forgesync, remove_archive, temporary_archive_path};

#[test]
fn archive_lifecycle_commands_call_the_store_and_return_versioned_json() {
    let path = temporary_archive_path();

    let init = forgesync()
        .args(["archive", "init", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run archive init");
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let init_json: serde_json::Value = serde_json::from_slice(&init.stdout).expect("init JSON");
    assert_eq!(init_json["command"], "archive init");
    assert_eq!(init_json["schema_version"], 1);
    assert_eq!(init_json["data"]["schema_version"], 10);
    let archive_id = init_json["data"]["archive_id"]
        .as_str()
        .expect("archive ID");
    assert_eq!(archive_id.len(), 36);

    let status = forgesync()
        .args(["archive", "status", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run archive status");
    assert!(
        status.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    let status_json: serde_json::Value =
        serde_json::from_slice(&status.stdout).expect("status JSON");
    assert_eq!(status_json["command"], "archive status");
    assert_eq!(status_json["data"]["archive"]["archive_id"], archive_id);
    assert_eq!(
        status_json["data"]["diagnostics"]["work"]["unresolved_failures"],
        0
    );
    assert_eq!(
        status_json["data"]["diagnostics"]["schema"]["history_valid"],
        true
    );

    let migrate = forgesync()
        .args(["archive", "migrate", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run archive migrate");
    assert!(migrate.status.success());
    let migrate_json: serde_json::Value =
        serde_json::from_slice(&migrate.stdout).expect("migration JSON");
    assert_eq!(migrate_json["command"], "archive migrate");
    assert_eq!(
        migrate_json["data"]["migration"]["applied_migrations"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let doctor = forgesync()
        .args(["archive", "doctor", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run archive doctor");
    assert!(
        doctor.status.success(),
        "{}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    let doctor_json: serde_json::Value =
        serde_json::from_slice(&doctor.stdout).expect("doctor JSON");
    assert_eq!(doctor_json["command"], "archive doctor");
    assert_eq!(doctor_json["data"]["healthy"], true);
    assert_eq!(doctor_json["data"]["checks"].as_array().unwrap().len(), 4);

    remove_archive(&path);
}
