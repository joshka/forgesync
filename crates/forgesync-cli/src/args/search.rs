//! Parsed search command arguments.

use clap::{ArgAction, Args};
use forgesync_engine::reference::RepositorySelector;

use super::{SearchModeArg, ThreadKindArg, ThreadSortArg, ThreadStateArg};

/// Search archived discussions with local keyword or optional semantic ranking.
#[derive(Clone, Debug, Args)]
pub struct SearchArgs {
    /// Search text sent to the selected local or semantic retrieval mode.
    pub query: String,
    /// Limit results to one or more registered repositories.
    #[arg(long = "repo", value_name = "OWNER/REPO")]
    pub repositories: Vec<RepositorySelector>,
    /// Limit results to issues or pull requests.
    #[arg(long, value_enum)]
    pub kind: Option<ThreadKindArg>,
    /// Filter by source open or closed state.
    #[arg(long, value_enum, default_value_t = ThreadStateArg::All)]
    pub state: ThreadStateArg,
    /// Choose keyword, semantic, hybrid, or explicit FTS5 retrieval.
    #[arg(long, value_enum, default_value_t = SearchModeArg::Keyword)]
    pub mode: SearchModeArg,
    /// Return keyword results if semantic retrieval has no compatible data or fails.
    #[arg(long, action = ArgAction::SetTrue)]
    pub keyword_fallback: bool,
    /// Sort results by relevance, source update time, or creation time.
    #[arg(long, value_enum)]
    pub sort: Option<ThreadSortArg>,
    /// Maximum number of results (1-1000).
    #[arg(
        long,
        default_value_t = 20,
        value_parser = clap::value_parser!(u32).range(1..=1000)
    )]
    pub limit: u32,
    /// Number of matching rows to skip.
    #[arg(long, default_value_t = 0)]
    pub offset: u64,
}
