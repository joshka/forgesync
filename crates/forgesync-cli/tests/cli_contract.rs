use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use assert_cmd::Command;
use forgesync_core::content::Repository;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;

static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

fn forgesync() -> Command {
    Command::new(env!("CARGO_BIN_EXE_forgesync"))
}

#[path = "cli_contract/archive.rs"]
mod archive;
#[path = "cli_contract/config.rs"]
mod config;
#[path = "cli_contract/process.rs"]
mod process;
#[path = "cli_contract/runs.rs"]
mod runs;
#[path = "cli_contract/status.rs"]
mod status;
#[path = "cli_contract/sync.rs"]
mod sync;

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
