//! Thread command arguments.

use clap::Subcommand;
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};

use super::{ThreadKindArg, ThreadSortArg, ThreadStateArg};

/// Local thread inspection operations.
#[derive(Clone, Debug, Subcommand)]
pub enum ThreadCommand {
    /// List discussions in stable update order.
    List {
        /// Limit results to one or more registered repositories.
        #[arg(long = "repo", value_name = "OWNER/REPO")]
        repositories: Vec<RepositorySelector>,
        /// Limit results to issues or pull requests.
        #[arg(long, value_enum)]
        kind: Option<ThreadKindArg>,
        /// Filter by source open or closed state.
        #[arg(long, value_enum, default_value_t = ThreadStateArg::All)]
        state: ThreadStateArg,
        /// Sort by source update or creation time.
        #[arg(long, value_enum)]
        sort: Option<ThreadSortArg>,
        /// Maximum number of results (1-1000).
        #[arg(
            long,
            default_value_t = 20,
            value_parser = clap::value_parser!(u32).range(1..=1000)
        )]
        limit: u32,
        /// Number of matching rows to skip.
        #[arg(long, default_value_t = 0)]
        offset: u64,
    },
    /// Show one discussion and its current selected evidence.
    Show {
        /// OWNER/REPO#NUMBER or a GitHub issue/pull-request URL.
        reference: ThreadSelector,
    },
}
