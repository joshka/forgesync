//! # Acquire selected GitHub evidence
//!
//! [`SyncArgs`] owns parsed repository/discussion scope and included evidence families. Its run
//! method installs cancellation, opens a writable archive, prepares the engine request, and
//! resolves only the hosts needed by that selection. All-repository selection reads the existing
//! archive registry; explicit selection retains the supplied selectors.
//!
//! The outer execution method owns archive closure exactly once after successful opening.
//! [`SyncFailure`] preserves selection, client-setup, and engine failures until cleanup is
//! complete. [`ProgressReporter`] drains advisory stderr snapshots before the terminal result is
//! rendered. Output mode, verbosity, and cancellation are process context, separate from the
//! acquisition scope.
//!
//! The report preserves per-thread and per-family failures. A partial run may still contain useful
//! committed observations, and output should show that distinction instead of collapsing the run
//! into one success flag.

use std::path::Path;
use std::process::ExitCode;

use clap::{ArgAction, Args};
use forgesync_engine::error::EngineError;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::sync::{SyncReport, SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use tokio_util::sync::CancellationToken;

use crate::command::github::{
    GitHubClientSetupError, github_clients_for_selectors, render_github_client_setup_error,
};
use crate::command::interruption::CommandInterruption;
use crate::command::progress::ProgressReporter;
use crate::command::values::{SyncIncludeArg, SyncThreadStateArg};
use crate::reports::{outcome_exit_code, sync_summary};
use crate::{OutputMode, render_engine_error, render_result, render_store_error};

/// Acquire GitHub discussions into the local archive.
#[derive(Clone, Debug, Args)]
pub struct SyncArgs {
    /// Repositories to sync; required unless `--all` is supplied.
    #[arg(
        value_name = "OWNER/REPO",
        required_unless_present = "all",
        conflicts_with = "all"
    )]
    pub repositories: Vec<RepositorySelector>,
    /// Sync every repository already registered in the archive.
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "repositories")]
    pub all: bool,
    /// Select open threads, closed threads, or a complete all-state enumeration.
    #[arg(long, value_enum)]
    pub state: Option<SyncThreadStateArg>,
    /// Add selected evidence families to the thread sync.
    #[arg(long = "with", value_enum, value_delimiter = ',')]
    pub with: Vec<SyncIncludeArg>,
}

impl SyncArgs {
    /// Runs the selected acquisition with process cancellation and closes before rendering.
    ///
    /// Archive opening is explicit and writable. The command closes it after either setup failure
    /// or engine completion; partial acquisition is rendered from its structured report.
    pub async fn run(self, path: &Path, output: OutputMode, verbose: u8) -> ExitCode {
        let interruption = CommandInterruption::new();
        self.execute(path, output, verbose, interruption.cancellation())
            .await
    }

    /// Owns the archive lifetime around request preparation and acquisition.
    async fn execute(
        self,
        path: &Path,
        output: OutputMode,
        verbose: u8,
        cancellation: &CancellationToken,
    ) -> ExitCode {
        let archive = match Archive::open_read_write(path).await {
            Ok(archive) => archive,
            Err(error) => return render_store_error(output, "sync", error),
        };
        let result = self.acquire(&archive, output, verbose, cancellation).await;
        archive.close().await;
        match result {
            Ok(report) => render_report(output, report),
            Err(error) => error.render(output),
        }
    }

    /// Resolves selected hosts, prepares clients, and drains advisory progress after execution.
    ///
    /// All-repository selection uses the archive registry before credential discovery. The engine
    /// still receives the original `all` request and owns its authoritative selection and ledger.
    async fn acquire(
        self,
        archive: &Archive,
        output: OutputMode,
        verbose: u8,
        cancellation: &CancellationToken,
    ) -> Result<SyncReport, SyncFailure> {
        let request = self.into_request();
        let selectors = selected_repositories(archive, &request).await?;
        let clients = github_clients_for_selectors(&selectors, verbose, cancellation)
            .await
            .map_err(SyncFailure::ClientSetup)?;
        let progress = ProgressReporter::start("sync", output, verbose);
        let result =
            sync_repositories(archive, &clients, &request, cancellation, progress.sender()).await;
        progress.finish().await;
        result.map_err(SyncFailure::Engine)
    }

    /// Converts parsed selection into domain scope without performing archive or provider I/O.
    fn into_request(self) -> SyncRequest {
        SyncRequest {
            repositories: self.repositories,
            all: self.all,
            scope: match self.state {
                None => SyncThreadScope::Default,
                Some(SyncThreadStateArg::Open) => SyncThreadScope::Open,
                Some(SyncThreadStateArg::Closed) => SyncThreadScope::Closed,
                Some(SyncThreadStateArg::All) => SyncThreadScope::All,
            },
            include_comments: self.with.contains(&SyncIncludeArg::Comments),
            include_reviews: self.with.contains(&SyncIncludeArg::Reviews),
            include_review_threads: self.with.contains(&SyncIncludeArg::ReviewThreads),
            parent_run: None,
        }
    }
}

/// Resolves the host-client scope from the archive registry or explicit parsed selectors.
async fn selected_repositories(
    archive: &Archive,
    request: &SyncRequest,
) -> Result<Vec<RepositorySelector>, SyncFailure> {
    if !request.all {
        return Ok(request.repositories.clone());
    }
    let registered = archive
        .list_repositories()
        .await
        .map_err(SyncFailure::Archive)?;
    Ok(registered
        .iter()
        .map(RepositorySelector::from_repository)
        .collect())
}

/// Renders structured acquisition outcomes after archive and progress cleanup have completed.
fn render_report(output: OutputMode, report: SyncReport) -> ExitCode {
    let exit_status = outcome_exit_code(&report.outcome);
    render_result(output, "sync", &report, sync_summary, exit_status)
}

/// Failed boundary retained until the outer command closes the archive.
#[derive(Debug, thiserror::Error)]
enum SyncFailure {
    /// Reading registered repository selection failed before client preparation.
    #[error("archive selection failed: {0}")]
    Archive(#[source] StoreError),
    /// Credential discovery or provider transport setup failed before acquisition.
    #[error("provider client setup failed: {0}")]
    ClientSetup(#[source] GitHubClientSetupError),
    /// Acquisition could not produce a structured report.
    #[error("acquisition failed: {0}")]
    Engine(#[source] EngineError),
}

impl SyncFailure {
    /// Preserves the established error envelope and exit policy after archive closure.
    fn render(self, output: OutputMode) -> ExitCode {
        match self {
            Self::Archive(error) => render_store_error(output, "sync", error),
            Self::ClientSetup(error) => render_github_client_setup_error(output, "sync", error),
            Self::Engine(error) => render_engine_error(output, "sync", error),
        }
    }
}

#[cfg(test)]
mod tests;
