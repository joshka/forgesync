//! # Cluster CLI contract
//!
//! These cases exercise generated cluster listing and local maintainer actions through the
//! executable. The distinction between machine proposals and recorded choices is part of the
//! command contract. Engine candidate tests and store decision tests cover the underlying analysis
//! and persistence.
//!
//! Each unavailable-target case names the first validation error: member actions resolve their
//! discussion before cluster validation. The offline build/list workflow proves no model key is
//! needed; threshold rejection is separate and happens before any archive is opened.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use assert_cmd::Command;
use forgesync_core::content::Repository;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId};
use forgesync_core::provider_data::ProviderData;
use forgesync_store::archive::Archive;

/// Gives concurrent scenarios distinct paths without relying on wall-clock timing.
static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

#[rstest::rstest]
#[case::dismiss(&["dismiss", "1"], "cluster dismiss", "cluster_missing")]
#[case::restore(&["restore", "1"], "cluster restore", "cluster_missing")]
#[case::exclude(&["exclude", "1", "owner/repo#1"], "cluster exclude", "repository_missing")]
#[case::include(&["include", "1", "owner/repo#1"], "cluster include", "repository_missing")]
#[case::canonical(&["canonical", "1", "owner/repo#1"], "cluster canonical", "repository_missing")]
#[tokio::test]
async fn decision_reports_unavailable_target_without_success_acknowledgment(
    #[case] arguments: &[&str],
    #[case] command: &str,
    #[case] code: &str,
) {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create empty archive");
    archive.close().await;

    let result = forgesync()
        .arg("--archive")
        .arg(&path)
        .args(["--json", "cluster"])
        .args(arguments)
        .output()
        .expect("run local decision");
    let envelope: serde_json::Value =
        serde_json::from_slice(&result.stdout).expect("decision failure JSON");

    assert_eq!(result.status.code(), Some(1));
    assert_eq!(envelope["command"], command);
    assert_eq!(envelope["error"]["code"], code);
    assert!(envelope.get("data").is_none());
    archive_file_cleanup(&path);
}

#[tokio::test]
async fn cluster_build_and_list_work_offline_without_embedding_credentials() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    archive
        .upsert_repository(&repository())
        .await
        .expect("register repository");
    archive.close().await;

    let build = forgesync()
        .args(["--archive"])
        .arg(&path)
        .args([
            "--json",
            "cluster",
            "build",
            "owner/repo",
            "--endpoint",
            "http://127.0.0.1:1/v1",
        ])
        .env_remove("OPENAI_API_KEY")
        .output()
        .expect("run offline cluster build");
    assert!(
        build.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let built: serde_json::Value = serde_json::from_slice(&build.stdout).expect("build JSON");
    assert_eq!(built["command"], "cluster build");
    assert_eq!(built["data"]["eligible_threads"], 0);
    assert_eq!(built["data"]["vector_threads"], 0);
    assert_eq!(built["data"]["generation"]["complete_coverage"], true);

    let list = forgesync()
        .args(["--archive"])
        .arg(&path)
        .args(["--json", "cluster", "list"])
        .output()
        .expect("list offline clusters");
    assert!(list.status.success());
    let listed: serde_json::Value = serde_json::from_slice(&list.stdout).expect("list JSON");
    assert_eq!(listed["command"], "cluster list");
    assert_eq!(listed["data"]["items"].as_array().unwrap().len(), 0);

    archive_file_cleanup(&path);
}

#[test]
fn invalid_threshold_is_rejected_before_archive_opening() {
    let path = temporary_archive_path();
    let invalid = forgesync()
        .args(["--archive"])
        .arg(&path)
        .args([
            "--json",
            "cluster",
            "build",
            "owner/repo",
            "--endpoint",
            "http://127.0.0.1:1/v1",
            "--threshold",
            "1.5",
        ])
        .output()
        .expect("reject invalid cluster threshold");
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("threshold"));

    assert!(!path.exists());
}

/// Constructs the built executable; each scenario states its own arguments and environment.
fn forgesync() -> Command {
    Command::new(env!("CARGO_BIN_EXE_forgesync"))
}

/// Supplies the registered repository for the offline build, with no discussions or vectors.
fn repository() -> Repository {
    Repository {
        id: RepositoryId::new(
            GitHubHost::parse("github.com").expect("host"),
            ProviderId::new("repo-cli-cluster").expect("repository provider ID"),
        ),
        owner: "owner".to_owned(),
        name: "repo".to_owned(),
        full_name: "owner/repo".to_owned(),
        default_branch: Some("main".to_owned()),
        updated_at: None,
        provider_data: ProviderData::new(),
    }
}

/// Reserves a process-local unique path without creating an archive.
fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-cli-clusters-{}-{sequence}.sqlite",
        std::process::id()
    ))
}

/// Removes the database and fixed SQLite sidecars after the scenario closes its handles.
/// The loop is resource cleanup rather than hidden scenario selection.
fn archive_file_cleanup(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
}
