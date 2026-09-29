//! # Parse and dispatch user commands
//!
//! Each command module owns its argument type and the method that runs it. Archive, thread,
//! search, run, cluster, sync, embed, and refresh tasks are separate user workflows; dispatch maps
//! a parsed variant to the owning implementation.
//!
//! `github` contains client setup shared by acquisition commands, and `values` maps CLI choices to
//! domain or engine values. Commands open the required archive mode, build typed requests, invoke
//! the engine or store, and pass results to `reports`. This boundary keeps Clap syntax and process
//! policy out of the libraries.

use std::path::PathBuf;

mod archive;
mod cluster;
mod embed;
mod embedding_service;
mod github;
mod interruption;
mod progress;
mod refresh;
mod retry;
mod run;
mod search;
mod sync;
mod thread;
#[cfg(feature = "tui")]
mod tui;
mod values;

pub use archive::ArchiveCommand;
pub use cluster::{ClusterBuildArgs, ClusterCommand, ClusterListArgs};
pub use embed::EmbedArgs;
pub use refresh::RefreshArgs;
pub use run::RunCommand;
pub use search::SearchArgs;
pub use sync::SyncArgs;
pub use thread::ThreadCommand;
pub use values::{
    ColorChoice, LogFormat, RefreshAnalysisArg, RunFamilyArg, SearchModeArg, SyncIncludeArg,
    SyncThreadStateArg, ThreadKindArg, ThreadSortArg, ThreadStateArg,
};

#[cfg(test)]
mod tests;

use std::process::ExitCode;

use clap::{ArgAction, Parser, Subcommand};
#[cfg(feature = "tui")]
use tui::run_tui;

use crate::config::ForgesyncConfig;
use crate::{OutputMode, usage_error};

/// Global process options shared by every command.
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
    Search(SearchArgs),
    /// Acquire GitHub discussions into the local archive.
    Sync(SyncArgs),
    /// Sync a repository and run explicitly selected local analysis stages.
    Refresh(RefreshArgs),
    /// Build deterministic documents and store compatible embeddings for local discussions.
    Embed(EmbedArgs),
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

impl CliArgs {
    /// Resolves process options and delegates the selected command to its owner.
    pub async fn dispatch(self, config: ForgesyncConfig) -> ExitCode {
        let Some(path) = self.archive else {
            return usage_error("--archive PATH is required for local archive commands");
        };
        let output = OutputMode::from(self.json);

        match self.command {
            Command::Archive { command } => command.run(&path, output).await,
            Command::Search(args) => {
                args.run(&path, output, config.embeddings, config.documents.recipe)
                    .await
            }
            Command::Sync(sync_args) => sync_args.run(&path, output, self.verbose).await,
            Command::Refresh(refresh_args) => {
                refresh_args.run(&path, output, self.verbose, config).await
            }
            Command::Embed(embed_args) => embed_args.run(&path, output, self.verbose, config).await,
            Command::Cluster { command } => command.run(&path, output, self.verbose, config).await,
            Command::Thread { command } => command.run(&path, output).await,
            Command::Run { command } => command.run(&path, output, self.verbose).await,
            #[cfg(feature = "tui")]
            Command::Tui => run_tui(&path, output, self.verbose).await,
        }
    }
}
