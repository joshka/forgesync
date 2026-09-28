use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use assert_cmd::Command;
use forgesync_core::{
    CollectionCompleteness, Discussion, EvidenceFamily, GitHubHost, Observation, ProviderData,
    ProviderId, Repository, RepositoryId, SourceClock, SourceState, ThreadId, ThreadKind,
    ThreadNumber, UtcTimestamp,
};
use forgesync_store::Archive;

static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

#[tokio::test]
async fn search_thread_inspect_and_status_run_offline_with_stable_json() {
    let path = temporary_archive_path();
    seed_archive(&path).await;

    let before = Archive::open_read_only(&path)
        .await
        .expect("open archive before local queries");
    let status_before = before.archive_status().await.expect("read status before");
    before.close().await;

    let ordinary_search = forgesync()
        .args(["--archive"])
        .arg(&path)
        .args([
            "--json",
            "search",
            "issues OR (cache*)",
            "--repo",
            "example/project",
        ])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .output()
        .expect("run ordinary search offline");
    assert!(
        ordinary_search.status.success(),
        "{}",
        String::from_utf8_lossy(&ordinary_search.stderr)
    );
    let ordinary_json: serde_json::Value =
        serde_json::from_slice(&ordinary_search.stdout).expect("ordinary search JSON");
    assert_eq!(ordinary_json["command"], "search");
    assert_eq!(ordinary_json["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        ordinary_json["data"]["items"][0]["thread"]["title"],
        "Issues OR cache timeout"
    );
    assert!(ordinary_json["data"]["coverage"].is_array());

    let advanced_search = forgesync()
        .args([
            "search",
            "issues OR cache",
            "--mode",
            "advanced-fts",
            "--archive",
        ])
        .arg(&path)
        .arg("--json")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .output()
        .expect("run advanced search offline");
    assert!(advanced_search.status.success());
    let advanced_json: serde_json::Value =
        serde_json::from_slice(&advanced_search.stdout).expect("advanced search JSON");
    assert_eq!(advanced_json["data"]["items"].as_array().unwrap().len(), 1);

    let thread_list = forgesync()
        .args(["--archive"])
        .arg(&path)
        .args([
            "thread",
            "list",
            "--repo",
            "example/project",
            "--kind",
            "issue",
            "--state",
            "open",
            "--json",
        ])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .output()
        .expect("run thread list offline");
    assert!(thread_list.status.success());
    let list_json: serde_json::Value =
        serde_json::from_slice(&thread_list.stdout).expect("thread list JSON");
    assert_eq!(list_json["command"], "thread list");
    assert_eq!(list_json["data"]["items"].as_array().unwrap().len(), 1);

    let thread_show = forgesync()
        .args(["thread", "show", "example/project#17", "--archive"])
        .arg(&path)
        .arg("--json")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .output()
        .expect("run thread show offline");
    assert!(thread_show.status.success());
    let show_json: serde_json::Value =
        serde_json::from_slice(&thread_show.stdout).expect("thread show JSON");
    assert_eq!(show_json["command"], "thread show");
    assert_eq!(
        show_json["data"]["summary"]["thread"]["title"],
        "Issues OR cache timeout"
    );

    let empty_search = forgesync()
        .args(["search", "missing-term", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run empty search");
    assert_eq!(empty_search.status.code(), Some(0));
    let empty_json: serde_json::Value =
        serde_json::from_slice(&empty_search.stdout).expect("empty search JSON");
    assert_eq!(empty_json["data"]["items"].as_array().unwrap().len(), 0);
    assert!(empty_json["data"]["coverage"].is_array());

    let malformed_advanced = forgesync()
        .args(["search", "NEAR(", "--mode", "advanced-fts", "--archive"])
        .arg(&path)
        .arg("--json")
        .output()
        .expect("run malformed advanced search");
    assert_eq!(malformed_advanced.status.code(), Some(1));
    let malformed_json: serde_json::Value =
        serde_json::from_slice(&malformed_advanced.stdout).expect("query error JSON");
    assert_eq!(malformed_json["error"]["code"], "search_query_invalid");

    let invalid_reference = forgesync()
        .args(["thread", "show", "17", "--archive"])
        .arg(&path)
        .output()
        .expect("run invalid reference");
    assert_eq!(invalid_reference.status.code(), Some(2));

    let invalid_limit = forgesync()
        .args(["search", "issues", "--limit", "0", "--archive"])
        .arg(&path)
        .output()
        .expect("run invalid limit");
    assert_eq!(invalid_limit.status.code(), Some(2));

    let after = Archive::open_read_only(&path)
        .await
        .expect("open archive after local queries");
    let status_after = after.archive_status().await.expect("read status after");
    assert_eq!(status_after, status_before);
    after.close().await;
    remove_archive(&path);
}

async fn seed_archive(path: &PathBuf) {
    let archive = Archive::create(path).await.expect("create archive");
    let repository = Repository {
        id: RepositoryId::new(
            GitHubHost::parse("github.com").expect("host"),
            ProviderId::new("repository-17").expect("repository ID"),
        ),
        owner: "example".to_owned(),
        name: "project".to_owned(),
        full_name: "example/project".to_owned(),
        default_branch: Some("main".to_owned()),
        updated_at: Some(timestamp("2026-09-20T10:00:00Z")),
        provider_data: ProviderData::new(),
    };
    archive
        .upsert_repository(&repository)
        .await
        .expect("store repository");
    let thread_id = ThreadId::new(
        repository.id,
        ProviderId::new("thread-17").expect("thread ID"),
        ThreadNumber::new(17).expect("thread number"),
    );
    let updated_at = timestamp("2026-09-20T10:00:00Z");
    let sequence = archive
        .reserve_observation_sequence(updated_at)
        .await
        .expect("reserve sequence");
    let discussion = Discussion {
        id: thread_id,
        kind: ThreadKind::Issue,
        state: SourceState::Open,
        title: "Issues OR cache timeout".to_owned(),
        body: Some("A cache issue appears after a network timeout.".to_owned()),
        html_url: Some("https://github.com/example/project/issues/17".to_owned()),
        created_at: timestamp("2026-09-19T10:00:00Z"),
        updated_at,
        closed_at: None,
        labels: vec!["bug".to_owned()],
        assignees: vec!["maintainer".to_owned()],
        provider_data: ProviderData::new(),
    };
    archive
        .apply_thread_observation(&Observation::new(
            EvidenceFamily::Threads,
            discussion,
            SourceClock::from_raw(Some("2026-09-20T10:00:00Z")),
            updated_at,
            sequence,
            CollectionCompleteness::Complete,
        ))
        .await
        .expect("apply thread observation");
    archive.close().await;
}

fn forgesync() -> Command {
    Command::new(env!("CARGO_BIN_EXE_forgesync"))
}

fn timestamp(value: &str) -> UtcTimestamp {
    UtcTimestamp::parse(value).expect("valid timestamp")
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
