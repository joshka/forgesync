use super::*;

#[test]
fn explicit_config_is_loaded_and_invalid_config_uses_the_json_error_envelope() {
    let archive_path = temporary_archive_path();
    let config_path = archive_path.with_extension("toml");
    std::fs::write(&config_path, "[documents]\nrecipe = 'original_body'\n")
        .expect("write valid config");

    let valid = forgesync()
        .args(["archive", "init", "--archive"])
        .arg(&archive_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--json")
        .output()
        .expect("run with explicit config");
    assert!(valid.status.success());

    std::fs::write(&config_path, "[documents]\nrecipe = 'unknown'\n")
        .expect("write invalid config");
    let invalid = forgesync()
        .args(["archive", "status", "--archive"])
        .arg(&archive_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--json")
        .output()
        .expect("run with invalid config");
    assert_eq!(invalid.status.code(), Some(2));
    let error: serde_json::Value = serde_json::from_slice(&invalid.stdout).expect("error JSON");
    assert_eq!(error["error"]["code"], "config_invalid");

    remove_archive(&archive_path);
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

#[tokio::test]
async fn semantic_search_requires_current_vectors_and_fallback_is_explicit() {
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

    let unavailable = forgesync()
        .args(["search", "local query", "--mode", "semantic", "--archive"])
        .arg(&path)
        .arg("--json")
        .env_remove("OPENAI_API_KEY")
        .output()
        .expect("run semantic search without vectors");
    assert_eq!(unavailable.status.code(), Some(1));
    let unavailable_json: serde_json::Value =
        serde_json::from_slice(&unavailable.stdout).expect("semantic error JSON");
    assert_eq!(
        unavailable_json["error"]["code"],
        "semantic_vectors_unavailable"
    );

    let fallback = forgesync()
        .args([
            "search",
            "local query",
            "--mode",
            "hybrid",
            "--keyword-fallback",
            "--archive",
        ])
        .arg(&path)
        .arg("--json")
        .env_remove("OPENAI_API_KEY")
        .output()
        .expect("run hybrid search with explicit keyword fallback");
    assert!(fallback.status.success());
    let fallback_json: serde_json::Value =
        serde_json::from_slice(&fallback.stdout).expect("fallback JSON");
    assert_eq!(fallback_json["data"]["requested_mode"], "hybrid");
    assert_eq!(fallback_json["data"]["mode"], "keyword");
    assert_eq!(
        fallback_json["data"]["fallback_reason"],
        "semantic_vectors_unavailable"
    );
    assert!(fallback_json["data"]["coverage"].is_array());

    let invalid_fallback = forgesync()
        .args(["search", "local query", "--keyword-fallback", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("reject fallback on keyword mode");
    assert_eq!(invalid_fallback.status.code(), Some(2));
    let invalid_fallback_json: serde_json::Value =
        serde_json::from_slice(&invalid_fallback.stdout).expect("invalid fallback JSON");
    assert_eq!(
        invalid_fallback_json["error"]["code"],
        "search_fallback_mode_invalid"
    );

    remove_archive(&path);
}
