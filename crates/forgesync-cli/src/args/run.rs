//! Run command arguments.

use clap::Subcommand;

use super::RunFamilyArg;

/// Durable sync-run operations.
#[derive(Clone, Debug, Subcommand)]
pub enum RunCommand {
    /// List recent archive runs.
    List {
        /// Maximum number of recent runs to show (1-1000).
        #[arg(
            long,
            default_value_t = 50,
            value_parser = clap::value_parser!(u32).range(1..=1000)
        )]
        limit: u32,
    },
    /// Show one run with its jobs and failure ledger.
    Show {
        /// Positive archive-local run ID.
        id: u64,
    },
    /// Retry unresolved failures from one run.
    Retry {
        /// Positive archive-local run ID.
        id: u64,
        /// Limit retries to selected evidence families; repeat or comma-separate values.
        #[arg(long, value_enum, value_delimiter = ',')]
        family: Vec<RunFamilyArg>,
    },
}
