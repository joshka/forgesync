use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use assert_cmd::Command;

static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

fn forgesync() -> Command {
    Command::new(env!("CARGO_BIN_EXE_forgesync"))
}

#[test]
fn version_flag_prints_package_version() {
    let output = forgesync().arg("--version").output().expect("run binary");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("version is UTF-8");
    assert!(stdout.starts_with("forgesync "));
}

#[test]
fn help_lists_global_options_and_no_deferred_commands() {
    let output = forgesync().arg("--help").output().expect("run binary");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help is UTF-8");
    for option in ["--archive", "--config", "--json", "--color", "--log-format"] {
        assert!(stdout.contains(option), "help must contain {option}");
    }
    for deferred in ["portable", "cloud", "summarize", "code index", "serve"] {
        assert!(
            !stdout.contains(deferred),
            "help must not advertise {deferred}"
        );
    }
    assert!(stdout.contains("search"));
    assert!(stdout.contains("thread"));
}

#[test]
fn json_does_not_change_clap_usage_errors() {
    let output = forgesync().arg("--json").output().expect("run binary");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic is UTF-8");
    assert!(stderr.contains("requires a subcommand"));
}

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
    assert_eq!(init_json["data"]["schema_version"], 4);
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
    assert!(status.status.success());
    let status_json: serde_json::Value =
        serde_json::from_slice(&status.stdout).expect("status JSON");
    assert_eq!(status_json["command"], "archive status");
    assert_eq!(status_json["data"]["archive"]["archive_id"], archive_id);

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
    assert_eq!(doctor_json["data"]["checks"].as_array().unwrap().len(), 3);

    remove_archive(&path);
}

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

fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-cli-{}-{sequence}.sqlite",
        std::process::id()
    ))
}

fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
}
