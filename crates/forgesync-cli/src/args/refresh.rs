//! Parsed refresh command arguments.

use clap::{ArgAction, Args};
use forgesync_engine::reference::RepositorySelector;

use super::{RefreshAnalysisArg, SyncIncludeArg, SyncThreadStateArg};

/// Sync a repository and run explicitly selected local analysis stages.
#[derive(Clone, Debug, Args)]
pub struct RefreshArgs {
    /// Repository scope shared by sync, embedding, and clustering stages.
    #[arg(value_name = "OWNER/REPO", required = true)]
    pub repositories: Vec<RepositorySelector>,
    /// Skip GitHub acquisition and analyze only the local archive.
    #[arg(long, action = ArgAction::SetTrue)]
    pub no_sync: bool,
    /// Select open threads, closed threads, or a complete all-state enumeration.
    #[arg(long, value_enum)]
    pub state: Option<SyncThreadStateArg>,
    /// Add selected evidence families to the sync stage.
    #[arg(long = "with", value_enum, value_delimiter = ',')]
    pub with: Vec<SyncIncludeArg>,
    /// Explicitly select model-backed stages; clustering uses stored vectors.
    #[arg(long, value_enum, value_delimiter = ',')]
    pub analyze: Vec<RefreshAnalysisArg>,
    /// Force embedding requests even when compatible vectors are stored.
    #[arg(long, action = ArgAction::SetTrue)]
    pub force: bool,
}
