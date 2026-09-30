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
use forgesync_engine::inspect::{ThreadListRequest, list_threads, show_thread};
use forgesync_engine::reference::ThreadSelector;
use forgesync_store::archive::Archive;

use crate::command::thread_filters::ThreadFilterArgs;
use crate::reports::threads::{render_thread_detail, render_thread_page};
use crate::{OutputMode, render_engine_error, render_store_error};

/// Local thread inspection operations.
#[derive(Clone, Debug, Subcommand)]
pub enum ThreadCommand {
    /// List discussions in stable update order.
    List(ThreadFilterArgs),
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
            Self::List(args) => Self::list(path, output, args.into_request()).await,
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

impl ThreadFilterArgs {
    /// Builds an ordinary listing request while preserving the engine's default sort policy.
    fn into_request(self) -> ThreadListRequest {
        ThreadListRequest {
            filters: self.into_filters(),
        }
    }
}
