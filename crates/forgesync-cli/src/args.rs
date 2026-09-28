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
    /// Search archived discussions using local data only.
    Search {
        /// Ordinary text or an explicit FTS5 expression.
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
        /// Choose keyword tokenization or explicit FTS5 syntax.
        #[arg(long, value_enum, default_value_t = SearchModeArg::Keyword)]
        mode: SearchModeArg,
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
    /// Inspect archived discussions and current family coverage.
    Thread {
        /// Thread list or detail operation.
        #[command(subcommand)]
        command: ThreadCommand,
    },
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

    use super::{CliArgs, ColorChoice, Command, LogFormat, SyncIncludeArg};

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
    }
}
