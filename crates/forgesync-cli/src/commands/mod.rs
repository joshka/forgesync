//! Command execution and request construction.

use std::process::ExitCode;

use forgesync_core::content::ThreadKind;
use forgesync_engine::inspect::{ThreadFilters, ThreadSort, ThreadStateFilter};

use crate::args::{CliArgs, Command, ThreadKindArg, ThreadSortArg, ThreadStateArg};
use crate::config::ForgesyncConfig;
use crate::{OutputMode, usage_error};

mod archive;
mod cluster;
mod embed;
mod github;
mod refresh;
mod retry;
mod run;
mod search;
mod sync;
mod thread;
#[cfg(feature = "tui")]
mod tui;

use archive::archive_command;
use cluster::cluster_from_cli;
use embed::embed_from_cli;
use refresh::refresh_from_cli;
use run::run_command;
use search::search_command;
use sync::sync_from_cli;
use thread::thread_command;
#[cfg(feature = "tui")]
use tui::tui_command;

pub(super) async fn dispatch(args: CliArgs, config: ForgesyncConfig) -> ExitCode {
    let Some(path) = args.archive else {
        return usage_error("--archive PATH is required for local archive commands");
    };
    let output = OutputMode::from(args.json);

    match args.command {
        Command::Archive { command } => archive_command(&path, output, command).await,
        Command::Search(args) => {
            search_command(
                args,
                &path,
                output,
                config.embeddings,
                config.documents.recipe,
            )
            .await
        }
        Command::Sync(sync_args) => sync_from_cli(sync_args, &path, output, args.verbose).await,
        Command::Refresh(refresh_args) => {
            refresh_from_cli(refresh_args, &path, output, args.verbose, config).await
        }
        Command::Embed(embed_args) => {
            embed_from_cli(embed_args, &path, output, args.verbose, config).await
        }
        Command::Cluster { command } => {
            cluster_from_cli(command, &path, output, args.verbose, config).await
        }
        Command::Thread { command } => thread_command(&path, output, command).await,
        Command::Run { command } => run_command(&path, output, args.verbose, command).await,
        #[cfg(feature = "tui")]
        Command::Tui => tui_command(&path, output, args.verbose).await,
    }
}

pub(super) fn thread_filters(
    repositories: Vec<forgesync_engine::reference::RepositorySelector>,
    kind: Option<ThreadKindArg>,
    state: ThreadStateArg,
    sort: Option<ThreadSortArg>,
    limit: u32,
    offset: u64,
) -> ThreadFilters {
    ThreadFilters {
        repositories,
        kind: kind.map(|kind| match kind {
            ThreadKindArg::Issue => ThreadKind::Issue,
            ThreadKindArg::Pr => ThreadKind::PullRequest,
        }),
        state: match state {
            ThreadStateArg::All => ThreadStateFilter::All,
            ThreadStateArg::Open => ThreadStateFilter::Open,
            ThreadStateArg::Closed => ThreadStateFilter::Closed,
        },
        sort: sort.map(|sort| match sort {
            ThreadSortArg::Relevance => ThreadSort::Relevance,
            ThreadSortArg::Updated => ThreadSort::Updated,
            ThreadSortArg::Created => ThreadSort::Created,
        }),
        limit,
        offset,
    }
}
