//! Parsed sync command arguments.

use clap::{ArgAction, Args};
use forgesync_engine::reference::RepositorySelector;

use super::{SyncIncludeArg, SyncThreadStateArg};

/// Acquire GitHub discussions into the local archive.
#[derive(Clone, Debug, Args)]
pub struct SyncArgs {
    /// Repositories to sync; required unless `--all` is supplied.
    #[arg(
        value_name = "OWNER/REPO",
        required_unless_present = "all",
        conflicts_with = "all"
    )]
    pub repositories: Vec<RepositorySelector>,
    /// Sync every repository already registered in the archive.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "repositories")]
    pub all: bool,
    /// Select open threads, closed threads, or a complete all-state enumeration.
    #[arg(long, value_enum)]
    pub state: Option<SyncThreadStateArg>,
    /// Add selected evidence families to the thread sync.
    #[arg(long = "with", value_enum, value_delimiter = ',')]
    pub with: Vec<SyncIncludeArg>,
}
