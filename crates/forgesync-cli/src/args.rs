use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand, ValueEnum};

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

    use super::{CliArgs, ColorChoice, LogFormat};

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
}
