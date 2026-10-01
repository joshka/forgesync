//! # Construct independent offline archives
//!
//! `seed_archive` creates a repository and a complete issue observation with fixed source times.
//! Its title/body deliberately contain words used by literal and advanced search scenarios.
//! Comments remain missing, supplying a visible distinction in human coverage output.
//!
//! The fixture closes its writable handle before returning; scenarios open their own read handles
//! or invoke the compiled CLI directly. Provider clients and credentials are unnecessary.
//! Unique paths isolate concurrent cases. Cleanup removes the database and fixed WAL sidecars.
//! These helpers construct data and release resources; they never execute a scenario query.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use assert_cmd::Command;
use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::observation::ThreadObservation;
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;

/// Separates concurrently constructed archives without timestamp assumptions.
static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

/// Creates one repository and complete parent observation, then closes the writable handle.
/// Child coverage remains missing so presentation cases can distinguish it from parent
/// completeness.
pub async fn seed_archive(path: &PathBuf) {
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
        .apply_thread_observation(
            &ThreadObservation {
                discussion,
                observed_at: updated_at,
                sequence,
            },
            None,
        )
        .await
        .expect("apply thread observation");
    archive.close().await;
}

/// Constructs the compiled process with user config isolated; callers supply scenario arguments.
pub fn forgesync() -> Command {
    let config_root = std::env::temp_dir().join(format!("forgesync-config-{}", std::process::id()));
    let mut command = Command::new(env!("CARGO_BIN_EXE_forgesync"));
    command.env_remove("FORGESYNC_CONFIG");
    command.env("XDG_CONFIG_HOME", &config_root);
    command.env("APPDATA", config_root);
    command
}

/// Parses fixed fixture timestamps without consulting the process clock.
fn timestamp(value: &str) -> UtcTimestamp {
    UtcTimestamp::parse(value).expect("valid timestamp")
}

/// Reserves a process-unique path without creating a database.
pub fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-cli-{}-{sequence}.sqlite",
        std::process::id()
    ))
}

/// Removes the closed database and its fixed SQLite sidecars.
/// The loop releases resources rather than selecting test scenarios.
pub fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
}
