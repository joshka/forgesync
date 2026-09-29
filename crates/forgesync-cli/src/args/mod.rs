use std::path::PathBuf;

mod archive;
mod cluster;
mod run;
mod thread;
mod values;

pub use archive::ArchiveCommand;
pub use cluster::ClusterCommand;
pub use run::RunCommand;
pub use thread::ThreadCommand;
pub use values::{
    ColorChoice, LogFormat, RefreshAnalysisArg, RunFamilyArg, SearchModeArg, SyncIncludeArg,
    SyncThreadStateArg, ThreadKindArg, ThreadSortArg, ThreadStateArg,
};

#[cfg(test)]
mod tests;

use clap::{ArgAction, Parser, Subcommand};
use forgesync_engine::reference::RepositorySelector;

/// Global process options. Command-specific arguments are added with their implementation phase.
#[derive(Clone, Debug, Parser)]
#[command(
    name = "forgesync",
    version,
    about = "A local, queryable GitHub archive",
    long_about = "Forgesync archives GitHub discussions locally for offline search and maintainer triage.",
    subcommand_required = true
)]
pub struct CliArgs {
    /// Select the archive path for this invocation.
    #[arg(long, global = true, value_name = "PATH")]
    pub archive: Option<PathBuf>,

    /// Select the TOML configuration file.
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Write machine-readable results as a versioned JSON envelope.
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    pub json: bool,

    /// Select terminal color behavior.
    #[arg(long, global = true, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,

    /// Select diagnostic log format independently of result output.
    #[arg(long, global = true, value_enum, default_value_t = LogFormat::Text)]
    pub log_format: LogFormat,

    /// Increase diagnostic verbosity. Use twice for debug output.
    #[arg(short, long, global = true, action = ArgAction::Count)]
    pub verbose: u8,

    /// Command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Top-level commands available in the selected local workflow.
#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Create and inspect the local archive.
    Archive {
        /// Archive lifecycle operation.
        #[command(subcommand)]
        command: ArchiveCommand,
    },
    /// Search archived discussions with local keyword or optional semantic ranking.
    Search {
        /// Search text sent to the selected local or semantic retrieval mode.
        query: String,
        /// Limit results to one or more registered repositories.
        #[arg(long = "repo", value_name = "OWNER/REPO")]
        repositories: Vec<RepositorySelector>,
        /// Limit results to issues or pull requests.
        #[arg(long, value_enum)]
        kind: Option<ThreadKindArg>,
        /// Filter by source open or closed state.
        #[arg(long, value_enum, default_value_t = ThreadStateArg::All)]
        state: ThreadStateArg,
        /// Choose keyword, semantic, hybrid, or explicit FTS5 retrieval.
        #[arg(long, value_enum, default_value_t = SearchModeArg::Keyword)]
        mode: SearchModeArg,
        /// Return keyword results if semantic retrieval has no compatible data or fails.
        #[arg(long, action = ArgAction::SetTrue)]
        keyword_fallback: bool,
        /// Sort results by relevance, source update time, or creation time.
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
    /// Acquire GitHub discussions into the local archive.
    Sync {
        /// Repositories to sync; required unless `--all` is supplied.
        #[arg(
            value_name = "OWNER/REPO",
            required_unless_present = "all",
            conflicts_with = "all"
        )]
        repositories: Vec<RepositorySelector>,
        /// Sync every repository already registered in the archive.
        #[arg(long, action = ArgAction::SetTrue, conflicts_with = "repositories")]
        all: bool,
        /// Select open threads, closed threads, or a complete all-state enumeration.
        #[arg(long, value_enum)]
        state: Option<SyncThreadStateArg>,
        /// Add selected evidence families to the thread sync.
        #[arg(long = "with", value_enum, value_delimiter = ',')]
        with: Vec<SyncIncludeArg>,
    },
    /// Sync a repository and run explicitly selected local analysis stages.
    Refresh {
        /// Repository scope shared by sync, embedding, and clustering stages.
        #[arg(value_name = "OWNER/REPO", required = true)]
        repositories: Vec<RepositorySelector>,
        /// Skip GitHub acquisition and analyze only the local archive.
        #[arg(long, action = ArgAction::SetTrue)]
        no_sync: bool,
        /// Select open threads, closed threads, or a complete all-state enumeration.
        #[arg(long, value_enum)]
        state: Option<SyncThreadStateArg>,
        /// Add selected evidence families to the sync stage.
        #[arg(long = "with", value_enum, value_delimiter = ',')]
        with: Vec<SyncIncludeArg>,
        /// Explicitly select model-backed stages; clustering uses stored vectors.
        #[arg(long, value_enum, value_delimiter = ',')]
        analyze: Vec<RefreshAnalysisArg>,
        /// Force embedding requests even when compatible vectors are stored.
        #[arg(long, action = ArgAction::SetTrue)]
        force: bool,
    },
    /// Build deterministic documents and store compatible embeddings for local discussions.
    Embed {
        /// One or more registered repositories to embed.
        #[arg(value_name = "OWNER/REPO", required = true)]
        repositories: Vec<RepositorySelector>,
        /// Force provider requests even when current compatible vectors are stored.
        #[arg(long, action = ArgAction::SetTrue)]
        force: bool,
        /// Override the configured OpenAI-compatible base endpoint.
        #[arg(long, value_name = "URL")]
        endpoint: Option<String>,
        /// Override the configured embedding model.
        #[arg(long, value_name = "MODEL")]
        model: Option<String>,
        /// Override the environment variable name containing the API key.
        #[arg(long, value_name = "NAME")]
        api_key_env: Option<String>,
        /// Override the expected output dimensions.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=65536))]
        dimensions: Option<u32>,
        /// Override the maximum UTF-8 bytes per input chunk.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=300000))]
        max_input_bytes: Option<u32>,
        /// Override the maximum UTF-8 bytes in one request.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=300000))]
        max_batch_input_bytes: Option<u32>,
        /// Override the maximum inputs per request.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=2048))]
        batch_size: Option<u32>,
        /// Override the maximum requests in flight for this service.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=64))]
        concurrency: Option<u32>,
    },
    /// Build related-discussion groups and apply local maintainer decisions.
    Cluster {
        /// Cluster generation, listing, inspection, or local decision.
        #[command(subcommand)]
        command: ClusterCommand,
    },
    /// Inspect archived discussions and current family coverage.
    Thread {
        /// Thread list or detail operation.
        #[command(subcommand)]
        command: ThreadCommand,
    },
    /// Inspect durable sync runs and retry unresolved work.
    Run {
        /// Durable run operation.
        #[command(subcommand)]
        command: RunCommand,
    },
    /// Browse and maintain the local archive in an interactive terminal.
    #[cfg(feature = "tui")]
    Tui,
}
