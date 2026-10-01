//! Acquire selected GitHub evidence.
//!
//! A partial run may still contain useful committed observations, so the report keeps per-thread
//! and per-family failures instead of collapsing the run into one success flag.

use std::path::Path;

use clap::{ArgAction, Args};
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::refresh::RefreshSyncOptions;
use forgesync_engine::sync::{SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;

use super::with_archive;
use crate::command::github::github_clients_for_selectors;
use crate::command::progress::ProgressReporter;
use crate::command::values::{SyncIncludeArg, SyncThreadStateArg};
use crate::error::{CliError, Exit};
use crate::output::Output;
use crate::reports::sync::{outcome_exit_code, sync_summary};

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
    #[command(flatten)]
    pub scope: SyncScopeArgs,
}

/// Thread state and evidence families shared by `sync` and `refresh`.
#[derive(Clone, Debug, Default, Args)]
pub struct SyncScopeArgs {
    /// Select open threads, closed threads, or a complete all-state enumeration.
    #[arg(long, value_enum)]
    pub state: Option<SyncThreadStateArg>,
    /// Add selected evidence families to the thread sync.
    #[arg(long = "with", value_enum, value_delimiter = ',')]
    pub with: Vec<SyncIncludeArg>,
}

impl SyncScopeArgs {
    pub fn options(&self) -> RefreshSyncOptions {
        RefreshSyncOptions {
            scope: match self.state {
                None => SyncThreadScope::Default,
                Some(SyncThreadStateArg::Open) => SyncThreadScope::Open,
                Some(SyncThreadStateArg::Closed) => SyncThreadScope::Closed,
                Some(SyncThreadStateArg::All) => SyncThreadScope::All,
            },
            include_comments: self.with.contains(&SyncIncludeArg::Comments),
            include_reviews: self.with.contains(&SyncIncludeArg::Reviews),
            include_review_threads: self.with.contains(&SyncIncludeArg::ReviewThreads),
        }
    }
}

impl SyncArgs {
    pub async fn run(
        self,
        path: &Path,
        output: Output,
        verbose: u8,
        cancellation: &CancellationToken,
    ) -> Result<Exit, CliError> {
        let request = self.into_request();
        let report = with_archive(Archive::open_read_write(path), async |archive| {
            let progress = ProgressReporter::start("sync", output.mode, verbose);
            // `--all` resolves hosts from the registry; the engine still owns the authoritative
            // selection for the run.
            let selectors = if request.all {
                archive
                    .list_repositories()
                    .await?
                    .iter()
                    .map(RepositorySelector::from_repository)
                    .collect()
            } else {
                request.repositories.clone()
            };
            let clients = github_clients_for_selectors(&selectors, verbose, cancellation).await?;
            let result =
                sync_repositories(archive, &clients, &request, cancellation, progress.sender())
                    .await;
            progress.finish().await;
            Ok::<_, CliError>(result?)
        })
        .await?;
        Ok(output.report(&report, sync_summary, outcome_exit_code(&report.outcome)))
    }

    fn into_request(self) -> SyncRequest {
        let RefreshSyncOptions {
            scope,
            include_comments,
            include_reviews,
            include_review_threads,
        } = self.scope.options();
        SyncRequest {
            repositories: self.repositories,
            all: self.all,
            scope,
            include_comments,
            include_reviews,
            include_review_threads,
            parent_run: None,
        }
    }
}

#[cfg(test)]
mod tests;
