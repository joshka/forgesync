use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use assert_cmd::Command;
use forgesync_core::content::Repository;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId};
use forgesync_core::provider_data::ProviderData;
use forgesync_store::Archive;

static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

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

    archive_file_cleanup(&path);
}

fn forgesync() -> Command {
    Command::new(env!("CARGO_BIN_EXE_forgesync"))
}

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

fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-cli-clusters-{}-{sequence}.sqlite",
        std::process::id()
    ))
}

fn archive_file_cleanup(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
}
