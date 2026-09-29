use super::*;

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
