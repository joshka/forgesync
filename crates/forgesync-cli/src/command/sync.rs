//! # Acquire selected GitHub evidence
//!
//! `SyncArgs` carries repository and discussion scope, included families, and process options. Its
//! run method resolves credentials and clients, opens a writable archive, then calls the engine
//! with an explicit sync request.
//!
//! The report preserves per-thread and per-family failures. A partial run may still contain useful
//! committed observations, and output should show that distinction instead of collapsing the run
//! into one success flag.

use std::process::ExitCode;

use clap::{ArgAction, Args};
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::sync::{SyncProgress, SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_store::archive::Archive;

use super::github::{github_clients_for_selectors, render_github_client_setup_error};
use super::{SyncIncludeArg, SyncThreadStateArg};
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

/// Opens an archive and runs one prepared acquisition request.
async fn execute_sync(
    archive_path: &std::path::Path,
    request: SyncRequest,
    json: OutputMode,
    verbose: u8,
    cancellation: &tokio_util::sync::CancellationToken,
) -> ExitCode {
    let archive = match Archive::open_read_write(archive_path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(json, "sync", error),
    };
    let selectors = if request.all {
        match archive.list_repositories().await {
            Ok(registered) => registered
                .iter()
                .map(RepositorySelector::from_repository)
                .collect::<Vec<_>>(),
            Err(error) => {
                archive.close().await;
                return render_store_error(json, "sync", error);
            }
        }
    } else {
        request.repositories.clone()
    };
    let clients = match github_clients_for_selectors(&selectors, verbose, cancellation).await {
        Ok(clients) => clients,
        Err(error) => {
            archive.close().await;
            return render_github_client_setup_error(json, "sync", error);
        }
    };

    let (progress_sender, mut progress_receiver) = tokio::sync::mpsc::channel::<SyncProgress>(4);
    let progress_task = if verbose > 0 && !json.is_json() {
        Some(tokio::spawn(async move {
            while let Some(progress) = progress_receiver.recv().await {
                let repository = progress.repository.as_deref().unwrap_or("sync");
                eprintln!(
                    "forgesync: {}: {:?}, {}/{} jobs, {} threads, {} comments, {} PRs, {} reviews, {} review threads",
                    repository,
                    progress.status,
                    progress.completed_jobs,
                    progress.total_jobs,
                    progress.threads_seen,
                    progress.comments_seen,
                    progress.pull_request_metadata_seen,
                    progress.reviews_seen,
                    progress.review_threads_seen
                );
            }
        }))
    } else {
        drop(progress_receiver);
        None
    };
    let result = sync_repositories(
        &archive,
        &clients,
        &request,
        cancellation,
        Some(progress_sender),
    )
    .await;
    if let Some(progress_task) = progress_task {
        let _ = progress_task.await;
    }
    archive.close().await;
    match result {
        Ok(report) => {
            let exit_status = outcome_exit_code(&report.outcome);
            render_result(json, "sync", &report, sync_summary, exit_status)
        }
        Err(error) => render_engine_error(json, "sync", error),
    }
}

impl SyncArgs {
    /// Converts parsed selection into a sync request and installs cancellation handling.
    pub async fn run(self, path: &std::path::Path, json: OutputMode, verbose: u8) -> ExitCode {
        let SyncArgs {
            repositories,
            all,
            state,
            with,
        } = self;
        let interruption = super::interruption::CommandInterruption::new();
        let cancellation = interruption.cancellation();
        let sync_request = SyncRequest {
            repositories,
            all,
            scope: match state {
                None => SyncThreadScope::Default,
                Some(SyncThreadStateArg::Open) => SyncThreadScope::Open,
                Some(SyncThreadStateArg::Closed) => SyncThreadScope::Closed,
                Some(SyncThreadStateArg::All) => SyncThreadScope::All,
            },
            include_comments: with.contains(&SyncIncludeArg::Comments),
            include_reviews: with.contains(&SyncIncludeArg::Reviews),
            include_review_threads: with.contains(&SyncIncludeArg::ReviewThreads),
            parent_run: None,
        };
        execute_sync(path, sync_request, json, verbose, cancellation).await
    }
}
