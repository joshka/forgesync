//! # Shared discussion filter arguments
//!
//! [`ThreadFilterArgs`] is the CLI-owned filter vocabulary shared by thread listing and search.
//! Clap flattens these fields into each command's flags, preserving a single set of names,
//! defaults, and basic numeric bounds. It does not own query text or retrieval mode.
//!
//! [`ThreadFilterArgs::into_filters`] converts parsed choices into engine request vocabulary.
//! The conversion performs no archive lookup or provider I/O and leaves absent sort policy for
//! the selected workflow to interpret. Engine validation still owns cross-field and SQLite offset
//! constraints; direct Rust construction does not receive Clap's parsing checks.
//!
//! Keeping this owner at the CLI boundary avoids positional bundles of repository, kind, state,
//! sort, limit, and offset while leaving the engine independent of Clap.

use clap::Args;
use forgesync_core::content::ThreadKind;
use forgesync_engine::inspect::{ThreadFilters, ThreadSort, ThreadStateFilter};
use forgesync_engine::reference::RepositorySelector;

use crate::command::values::{ThreadKindArg, ThreadSortArg, ThreadStateArg};

/// Parsed scope, ordering, and page arguments for discussion browsing or search.
#[derive(Clone, Debug, Args)]
pub struct ThreadFilterArgs {
    /// Limit results to one or more registered repositories.
    #[arg(long = "repo", value_name = "OWNER/REPO")]
    pub repositories: Vec<RepositorySelector>,
    /// Limit results to issues or pull requests.
    #[arg(long, value_enum)]
    pub kind: Option<ThreadKindArg>,
    /// Filter by source open or closed state.
    #[arg(long, value_enum, default_value_t = ThreadStateArg::All)]
    pub state: ThreadStateArg,
    /// Sort by source update or creation time.
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

impl ThreadFilterArgs {
    /// Converts parsed values without resolving repositories or selecting a default sort.
    pub fn into_filters(self) -> ThreadFilters {
        ThreadFilters {
            repositories: self.repositories,
            kind: self.kind.map(|kind| match kind {
                ThreadKindArg::Issue => ThreadKind::Issue,
                ThreadKindArg::Pr => ThreadKind::PullRequest,
            }),
            state: match self.state {
                ThreadStateArg::All => ThreadStateFilter::All,
                ThreadStateArg::Open => ThreadStateFilter::Open,
                ThreadStateArg::Closed => ThreadStateFilter::Closed,
            },
            sort: self.sort.map(|sort| match sort {
                ThreadSortArg::Relevance => ThreadSort::Relevance,
                ThreadSortArg::Updated => ThreadSort::Updated,
                ThreadSortArg::Created => ThreadSort::Created,
            }),
            limit: self.limit,
            offset: self.offset,
        }
    }
}
