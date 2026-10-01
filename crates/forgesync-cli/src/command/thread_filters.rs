//! Filter arguments shared by thread listing and search.
//!
//! The engine still validates cross-field and SQLite offset constraints; an absent sort is left for
//! the selected workflow to choose.

use clap::Args;
use forgesync_core::content::ThreadKind;
use forgesync_engine::inspect::{ThreadFilters, ThreadSort, ThreadStateFilter};
use forgesync_engine::reference::RepositorySelector;

use crate::command::values::{ThreadKindArg, ThreadSortArg, ThreadStateArg};

/// Repository, kind, state, sort, and page arguments for listing or search.
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
    /// Sort by relevance, source update time, or creation time.
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
    /// Converts to engine filters; an absent sort stays absent for the workflow to choose.
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

#[cfg(test)]
#[path = "thread_filters/tests.rs"]
mod tests;
