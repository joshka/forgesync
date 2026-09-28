use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use assert_cmd::Command;
use forgesync_core::UtcTimestamp;
use forgesync_store::Archive;

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
    assert_eq!(init_json["data"]["schema_version"], 7);
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

#[test]
fn sync_all_runs_against_the_registered_archive_and_emits_a_report() {
    let path = temporary_archive_path();
    let init = forgesync()
        .args(["archive", "init", "--archive"])
        .arg(&path)
        .output()
        .expect("initialize archive");
    assert!(init.status.success());

    let sync = forgesync()
        .args(["sync", "--all", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run sync");
    assert!(
        sync.status.success(),
        "{}",
        String::from_utf8_lossy(&sync.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&sync.stdout).expect("sync JSON");
    assert_eq!(report["command"], "sync");
    assert_eq!(report["data"]["outcome"]["status"], "complete");
    assert_eq!(report["data"]["repositories_selected"], 0);
    assert_eq!(report["data"]["total_jobs"], 0);

    remove_archive(&path);
}

#[test]
fn sync_requires_a_scope_and_rejects_all_with_explicit_repositories() {
    let missing_scope = forgesync()
        .args(["sync", "--archive", "missing.sqlite"])
        .output()
        .expect("run sync without scope");
    assert_eq!(missing_scope.status.code(), Some(2));

    let conflicting_scope = forgesync()
        .args(["sync", "owner/repo", "--all", "--archive", "missing.sqlite"])
        .output()
        .expect("run conflicting sync scope");
    assert_eq!(conflicting_scope.status.code(), Some(2));
}

#[tokio::test]
async fn another_process_cannot_start_a_mutating_sync_while_the_archive_is_leased() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after Unix epoch");
    let now = UtcTimestamp::from_unix_microseconds(
        i64::try_from(elapsed.as_micros()).expect("timestamp fits archive"),
    )
    .expect("valid timestamp");
    let lease = archive
        .acquire_archive_lease(now, Duration::from_secs(60))
        .await
        .expect("claim archive lease");

    let output = forgesync()
        .args(["sync", "--all", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run competing sync process");
    assert_eq!(output.status.code(), Some(1));
    let error: serde_json::Value = serde_json::from_slice(&output.stdout).expect("error JSON");
    assert_eq!(error["error"]["code"], "archive_lease_held");

    archive
        .release_archive_lease(&lease, now)
        .await
        .expect("release archive lease");
    archive.close().await;
    remove_archive(&path);
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
