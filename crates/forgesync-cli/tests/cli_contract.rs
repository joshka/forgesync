use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use assert_cmd::Command;
use forgesync_core::{
    GitHubHost, ProviderData, ProviderId, Repository, RepositoryId, UtcTimestamp,
};
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
    assert!(stdout.contains("--archive"));
    assert!(stdout.contains("--config"));
    assert!(stdout.contains("--json"));
    assert!(stdout.contains("--color"));
    assert!(stdout.contains("--log-format"));
    assert!(!stdout.contains("portable"));
    assert!(!stdout.contains("cloud"));
    assert!(!stdout.contains("summarize"));
    assert!(!stdout.contains("code index"));
    assert!(!stdout.contains("serve"));
    assert!(stdout.contains("search"));
    assert!(stdout.contains("thread"));
    assert!(stdout.contains("run"));
    assert!(stdout.contains("embed"));
    assert!(stdout.contains("cluster"));
    assert!(stdout.contains("tui"));
}

#[test]
fn json_does_not_change_clap_usage_errors() {
    let output = forgesync().arg("--json").output().expect("run binary");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic is UTF-8");
    assert!(stderr.contains("requires a subcommand"));
}

#[tokio::test]
async fn tui_requires_an_interactive_terminal() {
    let path = temporary_archive_path();
    let config_path = path.with_extension("toml");
    std::fs::write(&config_path, "[documents]\nrecipe = 'unknown'\n")
        .expect("write irrelevant invalid config");
    let archive = Archive::create(&path).await.expect("create archive");
    archive.close().await;

    let output = forgesync()
        .args(["tui", "--archive"])
        .arg(&path)
        .arg("--config")
        .arg(&config_path)
        .output()
        .expect("run tui without a terminal");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).expect("diagnostic is UTF-8");
    assert!(stderr.contains("requires an interactive terminal"));
    assert!(!stderr.contains("config file"));
    remove_archive(&path);
    let _ = std::fs::remove_file(config_path);
}

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

#[test]
fn run_list_show_and_retry_use_durable_run_records() {
    let path = temporary_archive_path();
    let init = forgesync()
        .args(["archive", "init", "--archive"])
        .arg(&path)
        .output()
        .expect("initialize archive");
    assert!(init.status.success());

    let list = forgesync()
        .args(["run", "list", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("list runs");
    assert!(list.status.success());
    let list_json: serde_json::Value = serde_json::from_slice(&list.stdout).expect("run list JSON");
    assert_eq!(list_json["command"], "run list");
    assert_eq!(list_json["data"], serde_json::json!([]));

    let show = forgesync()
        .args(["run", "show", "1", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("show run");
    assert_eq!(show.status.code(), Some(1));
    let show_json: serde_json::Value = serde_json::from_slice(&show.stdout).expect("run show JSON");
    assert_eq!(show_json["error"]["code"], "run_missing");

    let retry = forgesync()
        .args(["run", "retry", "1", "--family", "comments", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("retry run");
    assert_eq!(retry.status.code(), Some(1));
    let retry_json: serde_json::Value =
        serde_json::from_slice(&retry.stdout).expect("run retry JSON");
    assert_eq!(retry_json["error"]["code"], "run_missing");
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
