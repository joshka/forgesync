//! # Inspect archived discussions
//!
//! [`ThreadCommand`] groups list and show operations over existing local content. List converts
//! CLI repository, kind, state, ordering, and page choices into an engine inspection request.
//! Show accepts a parsed discussion selector and asks the engine to resolve it against registered
//! local repositories. Neither operation acquires content or resolves the selector through GitHub.
//!
//! Both operations open an existing archive read-only and close it before rendering the result or
//! an engine error. Opening does not create or migrate the archive. The engine and store own
//! filtering, identity resolution, ordering, and read projections; this module adapts arguments
//! and chooses the output presentation.
//!
//! Detail results include canonical discussion data and family coverage. Retained child evidence
//! can be incomplete or stale, so displaying a discussion is not a claim that all its evidence is
//! current. The thread report module owns human-readable layouts, while JSON preserves the engine
//! result through the shared CLI output boundary.

use std::path::Path;
use std::process::ExitCode;

use clap::Subcommand;
use forgesync_core::content::ThreadKind;
use forgesync_engine::inspect::{
    ThreadFilters, ThreadListRequest, ThreadSort, ThreadStateFilter, list_threads, show_thread,
};
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};
use forgesync_store::archive::Archive;

use crate::command::values::{ThreadKindArg, ThreadSortArg, ThreadStateArg};
use crate::reports::threads::{render_thread_detail, render_thread_page};
use crate::{OutputMode, render_engine_error, render_store_error};

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

impl ThreadCommand {
    /// Runs a local read and renders the selected thread result.
    ///
    /// Both variants open the existing archive read-only and close it before rendering.
    pub async fn run(self, path: &Path, output: OutputMode) -> ExitCode {
        match self {
            Self::List {
                repositories,
                kind,
                state,
                sort,
                limit,
                offset,
            } => {
                let request = ThreadListRequest {
                    filters: thread_filters(repositories, kind, state, sort, limit, offset),
                };
                Self::list(path, output, request).await
            }
            Self::Show { reference } => Self::show(path, output, reference).await,
        }
    }

    /// Applies list filters and renders one page from the local archive.
    async fn list(path: &Path, output: OutputMode, request: ThreadListRequest) -> ExitCode {
        let archive = match Archive::open_read_only(path).await {
            Ok(archive) => archive,
            Err(error) => return render_store_error(output, "thread list", error),
        };
        let result = list_threads(&archive, &request).await;
        archive.close().await;
        match result {
            Ok(page) => render_thread_page(output, "thread list", &page),
            Err(error) => render_engine_error(output, "thread list", error),
        }
    }

    /// Resolves one thread selector and renders its archived evidence.
    async fn show(path: &Path, output: OutputMode, reference: ThreadSelector) -> ExitCode {
        let archive = match Archive::open_read_only(path).await {
            Ok(archive) => archive,
            Err(error) => return render_store_error(output, "thread show", error),
        };
        let result = show_thread(&archive, &reference).await;
        archive.close().await;
        match result {
            Ok(detail) => render_thread_detail(output, &detail),
            Err(error) => render_engine_error(output, "thread show", error),
        }
    }
}

/// Converts parsed thread options to an engine read filter.
pub fn thread_filters(
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
