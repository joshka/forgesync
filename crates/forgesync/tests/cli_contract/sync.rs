//! # Sync command contract
//!
//! These cases distinguish empty registered scope, invalid argument selection, and a competing
//! writer lease. Empty `--all` succeeds with zero selected repositories and jobs; it does not
//! establish provider pagination or successful acquisition of any repository.
//!
//! Scope rejection occurs at parsing. The lease case creates a real current-time lease before
//! invoking a second process, then releases it explicitly during cleanup. Its wall-clock setup is
//! necessary because the competing process validates expiry using its own clock.
//! A Unix helper fixture returns deliberately malformed, credential-free output. Sync must report
//! its typed setup error before any provider request, proving subprocess I/O on the CLI runtime.
//! Engine/provider/store suites cover pagination, partial failures, and durable observation rules.
//! These process cases own selection, reported counts, and the visible writer-conflict diagnostic.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;

use super::{forgesync, remove_archive, temporary_archive_path};

#[tokio::test]
async fn sync_all_with_no_registered_repositories_returns_zero_work_report() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create empty archive");
    archive.close().await;

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
    assert!(sync.stderr.is_empty(), "JSON must suppress progress");
    assert_eq!(report["command"], "sync");
    assert_eq!(report["data"]["outcome"]["status"], "complete");
    assert_eq!(report["data"]["repositories_selected"], 0);
    assert_eq!(report["data"]["total_jobs"], 0);

    remove_archive(&path);
}

#[test]
fn sync_requires_a_scope() {
    let missing_scope = forgesync()
        .args(["sync", "--archive", "missing.sqlite"])
        .output()
        .expect("run sync without scope");
    assert_eq!(missing_scope.status.code(), Some(2));
}

#[test]
fn sync_rejects_all_with_explicit_repositories() {
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

#[cfg(unix)]
#[tokio::test]
async fn sync_runs_credential_helper_on_the_process_runtime_without_provider_calls() {
    use std::os::unix::fs::PermissionsExt;

    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    archive.close().await;
    let helper_directory = path.with_extension("helper");
    std::fs::create_dir(&helper_directory).expect("create isolated helper directory");
    let helper = helper_directory.join("gh");
    std::fs::write(&helper, "#!/bin/sh\nprintf 'invalid token'\n").expect("write harmless helper");
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700))
        .expect("make helper executable");

    let output = forgesync()
        .env_remove("GITHUB_TOKEN")
        .env("PATH", &helper_directory)
        .args(["sync", "ratatui/ratatui", "--archive"])
        .arg(&path)
        .arg("--json")
        .timeout(Duration::from_secs(30))
        .output()
        .expect("run sync with isolated credential helper");

    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {} stdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let error: serde_json::Value = serde_json::from_slice(&output.stdout).expect("error JSON");
    assert_eq!(error["error"]["code"], "github_credential_invalid");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));

    std::fs::remove_dir_all(helper_directory).expect("remove helper fixture");
    remove_archive(&path);
}

#[tokio::test]
async fn default_sync_reports_startup_on_stderr_and_keeps_summary_on_stdout() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create empty archive");
    archive.close().await;

    let output = forgesync()
        .args(["sync", "--all", "--archive"])
        .arg(&path)
        .output()
        .expect("run default sync");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("sync: preparing acquisition (Ctrl-C to cancel)"));
    assert!(
        !stderr.contains('\u{1b}'),
        "redirected progress stays plain"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("preparing acquisition"));
    remove_archive(&path);
}

#[tokio::test]
async fn json_sync_with_json_logs_keeps_both_streams_structured() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create empty archive");
    archive.close().await;

    let output = forgesync()
        .args([
            "--json",
            "--log-format",
            "json",
            "-v",
            "sync",
            "--all",
            "--archive",
        ])
        .arg(&path)
        .output()
        .expect("run sync with structured diagnostics");

    assert!(output.status.success());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).expect("result JSON");
    assert_eq!(result["command"], "sync");
    let stderr = String::from_utf8(output.stderr).expect("diagnostics are UTF-8");
    assert!(stderr.contains("Forgesync command started"));
    assert!(!stderr.contains("preparing acquisition"));
    assert!(!stderr.contains('\u{1b}'));
    for line in stderr.lines() {
        serde_json::from_str::<serde_json::Value>(line).expect("each diagnostic is JSON");
    }
    remove_archive(&path);
}

#[cfg(unix)]
#[tokio::test]
async fn default_sync_reports_elapsed_wait_during_credential_discovery() {
    use std::os::unix::fs::PermissionsExt;

    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    archive.close().await;
    let helper_directory = path.with_extension("helper");
    std::fs::create_dir(&helper_directory).expect("create isolated helper directory");
    let helper = helper_directory.join("gh");
    std::fs::write(&helper, "#!/bin/sh\n/bin/sleep 3\nprintf 'invalid token'\n")
        .expect("write delayed credential helper");
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700))
        .expect("make helper executable");

    let output = forgesync()
        .env_remove("GITHUB_TOKEN")
        .env("PATH", &helper_directory)
        .args(["sync", "ratatui/ratatui", "--archive"])
        .arg(&path)
        .timeout(Duration::from_secs(30))
        .output()
        .expect("run sync with delayed credential helper");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("sync: preparing acquisition"), "{stderr}");
    assert!(
        stderr.contains("sync: still running (2s elapsed"),
        "{stderr}"
    );
    assert!(
        !stderr.contains("invalid token"),
        "credentials must stay private"
    );
    std::fs::remove_dir_all(helper_directory).expect("remove helper fixture");
    remove_archive(&path);
}
