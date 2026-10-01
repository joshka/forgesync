//! Parse and dispatch user commands.
//!
//! Each command module owns its argument type and the method that runs it. Commands return
//! `Result<Exit, CliError>`; dispatch renders failures once with the command's envelope name.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod archive;
mod cluster;
mod embed;
mod github;
mod progress;
mod refresh;
mod run;
mod search;
mod sync;
mod thread;
mod thread_filters;
#[cfg(feature = "tui")]
mod tui;
pub mod values;

use archive::ArchiveCommand;
use clap::{ArgAction, Parser, Subcommand};
use cluster::ClusterCommand;
use embed::EmbedArgs;
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use refresh::RefreshArgs;
use run::RunCommand;
use search::SearchArgs;
use sync::SyncArgs;
use thread::ThreadCommand;
use tokio_util::sync::CancellationToken;
use values::{ColorChoice, LogFormat};

use crate::config::ForgesyncConfig;
use crate::error::CliError;
use crate::output::{Output, OutputMode};

#[cfg(test)]
mod tests;

/// Parsed global options and the selected command for one process invocation.
#[derive(Clone, Debug, Parser)]
#[command(
    name = "forgesync",
    version,
    about = "A local, queryable GitHub archive",
    long_about = "Forgesync archives GitHub discussions locally for offline search and maintainer triage.",
    subcommand_required = true
)]
pub struct CliArgs {
    /// Override the configured archive path for this invocation.
    #[arg(long, global = true, value_name = "PATH")]
    pub archive: Option<PathBuf>,

    /// Select the TOML configuration file.
    #[arg(long, global = true, value_name = "PATH", env = "FORGESYNC_CONFIG")]
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

    #[command(subcommand)]
    pub command: Command,
}

/// Top-level commands available in the selected local workflow.
#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Create and inspect the local archive.
    Archive {
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
        #[command(subcommand)]
        command: ClusterCommand,
    },
    /// Inspect archived discussions and current family coverage.
    Thread {
        #[command(subcommand)]
        command: ThreadCommand,
    },
    /// Inspect durable sync runs and retry unresolved work.
    Run {
        #[command(subcommand)]
        command: RunCommand,
    },
    /// Browse and maintain the local archive in an interactive terminal.
    #[cfg(feature = "tui")]
    Tui,
}

impl CliArgs {
    /// Rejects `tui` without an interactive terminal before configuration is read.
    #[cfg(feature = "tui")]
    pub fn check_terminal(&self) -> Result<(), CliError> {
        if !matches!(self.command, Command::Tui) {
            return Ok(());
        }
        if self.json {
            return Err(CliError::Usage(
                "--json is not supported by the interactive tui command",
            ));
        }
        tui::check_terminal()
    }

    /// Runs the selected command against the resolved archive path and renders any failure.
    pub async fn dispatch(self, path: &Path, config: ForgesyncConfig) -> ExitCode {
        let output = Output {
            mode: OutputMode::from(self.json),
            command: self.command.name(),
        };
        let verbose = self.verbose;
        let cancellation = CancellationToken::new();
        if self.command.interruptible() {
            let requested = cancellation.clone();
            tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    requested.cancel();
                }
            });
        }
        let cancellation = &cancellation;

        let result = match self.command {
            Command::Archive { command } => command.run(path, output).await,
            Command::Search(args) => args.run(path, output, config, cancellation).await,
            Command::Sync(args) => args.run(path, output, verbose, cancellation).await,
            Command::Refresh(args) => args.run(path, output, verbose, config, cancellation).await,
            Command::Embed(args) => args.run(path, output, verbose, config, cancellation).await,
            Command::Cluster { command } => {
                command
                    .run(path, output, verbose, config, cancellation)
                    .await
            }
            Command::Thread { command } => command.run(path, output).await,
            Command::Run { command } => command.run(path, output, verbose, cancellation).await,
            #[cfg(feature = "tui")]
            Command::Tui => tui::run_tui(path, verbose).await,
        };
        match result {
            Ok(exit) => exit.into(),
            Err(error) => output.error(&error),
        }
    }
}

impl Command {
    /// Command path reported in JSON envelopes.
    fn name(&self) -> &'static str {
        match self {
            Self::Archive { command } => match command {
                ArchiveCommand::Init => "archive init",
                ArchiveCommand::Migrate => "archive migrate",
                ArchiveCommand::Status => "archive status",
                ArchiveCommand::Doctor => "archive doctor",
            },
            Self::Search(_) => "search",
            Self::Sync(_) => "sync",
            Self::Refresh(_) => "refresh",
            Self::Embed(_) => "embed",
            Self::Cluster { command } => match command {
                ClusterCommand::Build(_) => "cluster build",
                ClusterCommand::List(_) => "cluster list",
                ClusterCommand::Show { .. } => "cluster show",
                ClusterCommand::Dismiss { .. } => "cluster dismiss",
                ClusterCommand::Restore { .. } => "cluster restore",
                ClusterCommand::Exclude { .. } => "cluster exclude",
                ClusterCommand::Include { .. } => "cluster include",
                ClusterCommand::Canonical { .. } => "cluster canonical",
            },
            Self::Thread { command } => match command {
                ThreadCommand::List(_) => "thread list",
                ThreadCommand::Show { .. } => "thread show",
            },
            Self::Run { command } => match command {
                RunCommand::List { .. } => "run list",
                RunCommand::Show { .. } => "run show",
                RunCommand::Retry { .. } => "run retry",
            },
            #[cfg(feature = "tui")]
            Self::Tui => "tui",
        }
    }

    /// Whether Ctrl-C should cooperatively cancel the command instead of killing the process.
    fn interruptible(&self) -> bool {
        matches!(
            self,
            Self::Search(_)
                | Self::Sync(_)
                | Self::Refresh(_)
                | Self::Embed(_)
                | Self::Cluster {
                    command: ClusterCommand::Build(_)
                }
                | Self::Run {
                    command: RunCommand::Retry { .. }
                }
        )
    }
}

/// Runs `operation` on an archive opened by `open`, closing it before returning either outcome.
async fn with_archive<T, E>(
    open: impl Future<Output = Result<Archive, StoreError>>,
    operation: impl AsyncFnOnce(&Archive) -> Result<T, E>,
) -> Result<T, CliError>
where
    CliError: From<E>,
{
    let archive = open.await?;
    let result = operation(&archive).await;
    archive.close().await;
    Ok(result?)
}
