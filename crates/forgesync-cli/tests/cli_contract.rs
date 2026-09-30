//! # CLI process contract
//!
//! This suite groups process-level command behavior by area. Its child modules cover archive
//! lifecycle, configuration, status, sync, run history, and shell exit behavior. The tests invoke
//! the compiled CLI and inspect output, so they establish the user-visible contract rather than
//! implementation details of argument parsing.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use assert_cmd::Command;

/// Distinguishes archive paths within this test process without sharing database state.
static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

/// Builds a fresh process command for the workspace executable without running it.
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

/// Reserves a process-local unique filename; callers explicitly create and close the archive.
///
/// This does not create a file or configure the child process environment.
fn temporary_archive_path() -> PathBuf {
    let sequence = NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "forgesync-cli-{}-{sequence}.sqlite",
        std::process::id()
    ))
}

/// Removes a closed fixture archive and its fixed SQLite WAL/shared-memory sidecars.
///
/// Missing files are expected after failed creation or SQLite cleanup. The loop only releases
/// resources; it does not select scenarios or conceal test assertions.
fn remove_archive(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(sidecar));
    }
}
