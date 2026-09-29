use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use forgesync_engine::{RepositorySelector, ThreadSelector};

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

/// Explicit archive lifecycle operations.
#[derive(Clone, Debug, Subcommand)]
pub enum ArchiveCommand {
    /// Create a new archive without overwriting an existing file.
    Init,
    /// Apply pending schema migrations to an existing archive.
    Migrate,
    /// Show validated archive metadata without changing the archive.
    Status,
    /// Check archive integrity and required SQLite capabilities.
    Doctor,
}

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

/// Deterministic cluster generation, inspection, and local governance operations.
#[derive(Clone, Debug, Subcommand)]
pub enum ClusterCommand {
    /// Build groups from current open discussions and compatible stored vectors.
    Build {
        /// Registered repository to cluster.
        #[arg(value_name = "OWNER/REPO")]
        repository: RepositorySelector,
        /// Override the configured embedding base endpoint; no request is sent.
        #[arg(long, value_name = "URL")]
        endpoint: Option<String>,
        /// Override the configured embedding model identity.
        #[arg(long, value_name = "MODEL")]
        model: Option<String>,
        /// Minimum cosine similarity for a same-kind edge.
        #[arg(
            long,
            default_value_t = 0.80,
            value_parser = parse_unit_float
        )]
        threshold: f64,
        /// Minimum cosine similarity for an issue-to-pull-request edge.
        #[arg(
            long,
            default_value_t = 0.93,
            value_parser = parse_unit_float
        )]
        cross_kind_threshold: f64,
        /// Maximum retained neighbors per discussion (1-256).
        #[arg(long, default_value_t = 16, value_parser = clap::value_parser!(u32).range(1..=256))]
        fanout: u32,
        /// Maximum component size (1-10000).
        #[arg(long, default_value_t = 40, value_parser = clap::value_parser!(u32).range(1..=10000))]
        max_cluster_size: u32,
        /// Minimum component size to persist (1-10000).
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=10000))]
        min_cluster_size: u32,
    },
    /// List current clusters, optionally including those retired by a complete run.
    List {
        /// Limit results to registered repositories.
        #[arg(long = "repo", value_name = "OWNER/REPO")]
        repositories: Vec<RepositorySelector>,
        /// Include retired clusters.
        #[arg(long, action = ArgAction::SetTrue)]
        include_retired: bool,
        /// Maximum number of clusters (1-1000).
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u32).range(1..=1000))]
        limit: u32,
        /// Number of clusters to skip.
        #[arg(long, default_value_t = 0)]
        offset: u64,
    },
    /// Show one generated cluster and its current member decisions.
    Show {
        /// Positive archive-local cluster ID.
        #[arg(value_parser = clap::value_parser!(u64).range(1..))]
        id: u64,
    },
    /// Hide one generated cluster from local triage without changing GitHub.
    Dismiss {
        /// Positive archive-local cluster ID.
        #[arg(value_parser = clap::value_parser!(u64).range(1..))]
        id: u64,
        /// Optional maintainer rationale retained in the archive.
        #[arg(long)]
        reason: Option<String>,
    },
    /// Clear a local cluster dismissal.
    Restore {
        /// Positive archive-local cluster ID.
        #[arg(value_parser = clap::value_parser!(u64).range(1..))]
        id: u64,
    },
    /// Exclude one current member from a generated cluster.
    Exclude {
        /// Positive archive-local cluster ID.
        #[arg(value_parser = clap::value_parser!(u64).range(1..))]
        id: u64,
        /// OWNER/REPO#NUMBER or a GitHub issue/pull-request URL.
        member: ThreadSelector,
        /// Optional maintainer rationale retained in the archive.
        #[arg(long)]
        reason: Option<String>,
    },
    /// Include a previously excluded current member.
    Include {
        /// Positive archive-local cluster ID.
        #[arg(value_parser = clap::value_parser!(u64).range(1..))]
        id: u64,
        /// OWNER/REPO#NUMBER or a GitHub issue/pull-request URL.
        member: ThreadSelector,
    },
    /// Select the canonical discussion for one generated cluster.
    Canonical {
        /// Positive archive-local cluster ID.
        #[arg(value_parser = clap::value_parser!(u64).range(1..))]
        id: u64,
        /// OWNER/REPO#NUMBER or a GitHub issue/pull-request URL.
        member: ThreadSelector,
    },
}

fn parse_unit_float(value: &str) -> Result<f64, String> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| "value must be a number between 0 and 1".to_owned())?;
    if !parsed.is_finite() || !(0.0..=1.0).contains(&parsed) {
        return Err("value must be between 0 and 1".to_owned());
    }
    Ok(parsed)
}

/// Evidence family accepted by explicit run retry filters.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum RunFamilyArg {
    /// Repository discussion enumeration.
    Threads,
    /// Discussion comments.
    Comments,
    /// Pull-request base and head metadata.
    PullRequestMetadata,
    /// Submitted pull-request reviews.
    Reviews,
    /// Current pull-request review threads.
    ReviewThreads,
}

/// Discussion kind accepted by local query filters.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ThreadKindArg {
    /// GitHub issue.
    Issue,
    /// GitHub pull request.
    Pr,
}

/// Source state accepted by local query filters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum ThreadStateArg {
    /// Include open, closed, and unrecognized source states.
    #[default]
    All,
    /// Include source-open discussions.
    Open,
    /// Include source-closed discussions.
    Closed,
}

/// Sort order accepted by local query commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ThreadSortArg {
    /// Rank FTS matches first.
    Relevance,
    /// Sort by source update time, newest first.
    Updated,
    /// Sort by source creation time, newest first.
    Created,
}

/// Search expression grammar selected for one local query.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum SearchModeArg {
    /// Quote ordinary text tokens and treat punctuation as separators.
    #[default]
    Keyword,
    /// Rank current compatible document vectors by exact cosine similarity.
    Semantic,
    /// Fuse keyword and semantic result ranks.
    Hybrid,
    /// Accept FTS5 phrases, boolean operators, and grouping syntax.
    AdvancedFts,
}

/// Thread-state selection accepted by sync.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum SyncThreadStateArg {
    /// Fetch only open threads.
    Open,
    /// Fetch only closed threads, using the successful closed-sweep watermark.
    Closed,
    /// Fetch all open and closed threads.
    All,
}

/// Optional evidence family selected for a sync run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum SyncIncludeArg {
    /// Acquire issue and pull-request discussion comments.
    Comments,
    /// Acquire pull-request reviews and reviewer identities.
    Reviews,
    /// Acquire current pull-request review threads and nested comments through GraphQL.
    ReviewThreads,
}

/// Optional analysis stage accepted by refresh.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum RefreshAnalysisArg {
    /// Build current documents and request missing embedding vectors.
    #[value(name = "embeddings")]
    Embeddings,
    /// Generate deterministic clusters from compatible stored vectors.
    Clusters,
}

/// Terminal color selection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum ColorChoice {
    /// Detect whether stdout is a terminal and respect NO_COLOR.
    #[default]
    Auto,
    /// Always use terminal colors for human output.
    Always,
    /// Never use terminal colors.
    Never,
}

/// Diagnostic log encoding.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum LogFormat {
    /// Human-readable diagnostics.
    #[default]
    Text,
    /// Structured JSON diagnostics.
    Json,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{CliArgs, ColorChoice, Command, LogFormat, RefreshAnalysisArg, SyncIncludeArg};

    #[test]
    fn global_options_parse_together() {
        let args = CliArgs::try_parse_from([
            "forgesync",
            "--archive",
            "archive.db",
            "--config",
            "config.toml",
            "--json",
            "--color",
            "never",
            "--log-format",
            "json",
            "-vv",
            "archive",
            "status",
        ])
        .expect("global options should parse");

        assert_eq!(
            args.archive.as_deref().and_then(|path| path.to_str()),
            Some("archive.db")
        );
        assert_eq!(
            args.config.as_deref().and_then(|path| path.to_str()),
            Some("config.toml")
        );
        assert!(args.json);
        assert_eq!(args.color, ColorChoice::Never);
        assert_eq!(args.log_format, LogFormat::Json);
        assert_eq!(args.verbose, 2);
    }

    #[test]
    fn sync_families_are_selected_with_with() {
        let args =
            CliArgs::try_parse_from(["forgesync", "--archive", "archive.db", "sync", "owner/repo"])
                .expect("sync without comments should parse");
        assert!(matches!(
            args.command,
            Command::Sync {
                with,
                ..
            } if with.is_empty()
        ));

        let args = CliArgs::try_parse_from([
            "forgesync",
            "--archive",
            "archive.db",
            "sync",
            "owner/repo",
            "--with",
            "comments",
        ])
        .expect("sync with comments should parse");
        assert!(matches!(
            args.command,
            Command::Sync {
                with,
                ..
            } if with == vec![SyncIncludeArg::Comments]
        ));

        let args = CliArgs::try_parse_from([
            "forgesync",
            "--archive",
            "archive.db",
            "sync",
            "owner/repo",
            "--with",
            "comments,reviews",
        ])
        .expect("sync with reviews should parse");
        assert!(matches!(
            args.command,
            Command::Sync { with, .. }
                if with == vec![SyncIncludeArg::Comments, SyncIncludeArg::Reviews]
        ));

        let args = CliArgs::try_parse_from([
            "forgesync",
            "--archive",
            "archive.db",
            "sync",
            "owner/repo",
            "--with",
            "review-threads",
        ])
        .expect("sync with review threads should parse");
        assert!(matches!(
            args.command,
            Command::Sync { with, .. }
                if with == vec![SyncIncludeArg::ReviewThreads]
        ));
    }

    #[test]
    fn refresh_analysis_stages_are_explicit_and_comma_separated() {
        let args = CliArgs::try_parse_from([
            "forgesync",
            "--archive",
            "archive.db",
            "refresh",
            "owner/repo",
        ])
        .expect("sync-only refresh should parse");
        assert!(matches!(args.command, Command::Refresh { analyze, .. } if analyze.is_empty()));

        let args = CliArgs::try_parse_from([
            "forgesync",
            "--archive",
            "archive.db",
            "refresh",
            "owner/repo",
            "--no-sync",
            "--analyze",
            "embeddings,clusters",
        ])
        .expect("explicit analysis stages should parse");
        assert!(matches!(
            args.command,
            Command::Refresh { no_sync: true, analyze, .. }
                if analyze == vec![RefreshAnalysisArg::Embeddings, RefreshAnalysisArg::Clusters]
        ));
    }
}
