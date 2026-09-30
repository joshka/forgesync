//! # Search capability and fallback policy at the process boundary
//!
//! These cases distinguish unavailable semantic evidence, explicitly permitted hybrid fallback,
//! and invalid fallback selection for ordinary keyword mode. Each command states its own request
//! and expected mode or typed diagnostic rather than relying on an earlier command's result.
//!
//! The archive contains a registered repository with no discussions or vectors. Removing the
//! model key keeps capability failure separate from provider requests; no query vector is needed
//! when semantic candidates are unavailable. Engine suites cover compatible-vector ranking.
//! Each scenario closes construction handles before invoking the compiled process.

use forgesync_core::content::Repository;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId};
use forgesync_core::provider_data::ProviderData;
use forgesync_store::archive::Archive;

use crate::{forgesync, remove_archive, temporary_archive_path};

#[tokio::test]
async fn semantic_search_without_vectors_returns_unavailable() {
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

    remove_archive(&path);
}

#[tokio::test]
async fn hybrid_search_uses_explicit_keyword_fallback() {
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

    remove_archive(&path);
}

#[tokio::test]
async fn keyword_mode_rejects_semantic_fallback_option() {
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
