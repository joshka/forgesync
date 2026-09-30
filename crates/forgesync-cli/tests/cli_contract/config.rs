//! # Configuration command contract
//!
//! These cases show how CLI options, local settings, and environment inputs select a workflow.
//! They inspect process behavior at the boundary where library requests are constructed. Keep
//! expectations here when a setting changes user-visible precedence or diagnostics.
//!
//! Valid and invalid document recipes have separate scenarios, including rejection before archive
//! creation. Empty-repository embedding and cluster-only refresh exercise selected capabilities
//! without making provider requests. Search-specific fallback policies live in `search_policy`.
//! Archives are independent per case and their writable handles close before process execution.

use forgesync_core::content::Repository;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId};
use forgesync_core::provider_data::ProviderData;
use forgesync_store::archive::Archive;

use super::{forgesync, remove_archive, temporary_archive_path};

#[test]
fn explicit_valid_config_is_loaded_before_archive_creation() {
    let archive_path = temporary_archive_path();
    let config_path = archive_path.with_extension("toml");
    std::fs::write(&config_path, "[documents]\nrecipe = 'original_body'\n")
        .expect("write valid config");

    let result = forgesync()
        .args(["archive", "init", "--archive"])
        .arg(&archive_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--json")
        .output()
        .expect("run with explicit config");
    let envelope: serde_json::Value = serde_json::from_slice(&result.stdout).expect("init JSON");
    assert!(result.status.success());
    assert_eq!(envelope["command"], "archive init");
    assert!(archive_path.exists());

    remove_archive(&archive_path);
    let _ = std::fs::remove_file(config_path);
}

#[test]
fn invalid_config_returns_json_error_before_archive_creation() {
    let archive_path = temporary_archive_path();
    let config_path = archive_path.with_extension("toml");
    std::fs::write(&config_path, "[documents]\nrecipe = 'unknown'\n")
        .expect("write invalid config");

    let result = forgesync()
        .args(["archive", "init", "--archive"])
        .arg(&archive_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--json")
        .output()
        .expect("run with invalid config");
    let envelope: serde_json::Value = serde_json::from_slice(&result.stdout).expect("error JSON");
    assert_eq!(result.status.code(), Some(2));
    assert_eq!(envelope["error"]["code"], "config_invalid");
    assert!(!archive_path.exists());

    let _ = std::fs::remove_file(config_path);
}

#[tokio::test]
async fn embed_empty_registered_repository_needs_no_provider_request() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let repository_id = RepositoryId::new(
        GitHubHost::parse("github.com").expect("host"),
        ProviderId::new("41").expect("repository ID"),
    );
    archive
        .upsert_repository(&Repository {
            id: repository_id,
            owner: "owner".to_owned(),
            name: "repo".to_owned(),
            full_name: "owner/repo".to_owned(),
            default_branch: Some("main".to_owned()),
            updated_at: None,
            provider_data: ProviderData::new(),
        })
        .await
        .expect("register repository");
    archive.close().await;

    let output = forgesync()
        .args(["embed", "owner/repo", "--archive"])
        .arg(&path)
        .args(["--endpoint", "http://127.0.0.1:1/v1", "--json"])
        .output()
        .expect("run embed without selected threads");
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).expect("embed JSON");
    assert_eq!(result["command"], "embed");
    assert_eq!(result["data"]["report"]["documents"], 0);
    assert_eq!(result["data"]["report"]["chunks_selected"], 0);

    remove_archive(&path);
}

#[tokio::test]
async fn refresh_can_cluster_local_archive_without_reading_a_model_key() {
    let path = temporary_archive_path();
    let archive = Archive::create(&path).await.expect("create archive");
    let repository_id = RepositoryId::new(
        GitHubHost::parse("github.com").expect("host"),
        ProviderId::new("41").expect("repository ID"),
    );
    archive
        .upsert_repository(&Repository {
            id: repository_id,
            owner: "owner".to_owned(),
            name: "repo".to_owned(),
            full_name: "owner/repo".to_owned(),
            default_branch: Some("main".to_owned()),
            updated_at: None,
            provider_data: ProviderData::new(),
        })
        .await
        .expect("register repository");
    archive.close().await;

    let output = forgesync()
        .args([
            "refresh",
            "owner/repo",
            "--no-sync",
            "--analyze",
            "clusters",
            "--archive",
        ])
        .arg(&path)
        .arg("--json")
        .env_remove("OPENAI_API_KEY")
        .output()
        .expect("run local cluster refresh without an API key");
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).expect("refresh JSON");
    assert_eq!(result["data"]["selected"], serde_json::json!(["clusters"]));
    assert_eq!(result["data"]["clusters"]["status"], "complete");
    assert!(result["data"]["embeddings"].is_null());
    assert_eq!(result["data"]["remaining"], serde_json::json!([]));

    remove_archive(&path);
}
