//! Inspect archived discussions.
//!
//! Retained child evidence can be incomplete or stale, so showing a discussion is not a claim
//! that all of its evidence is current.

use std::path::Path;

use clap::Subcommand;
use forgesync_engine::inspect::{ThreadListRequest, list_threads, show_thread};
use forgesync_engine::reference::ThreadSelector;
use forgesync_store::archive::Archive;

use super::with_archive;
use crate::command::thread_filters::ThreadFilterArgs;
use crate::error::{CliError, Exit};
use crate::output::Output;
use crate::reports::threads::{render_thread_detail, render_thread_page};

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
    pub async fn run(self, path: &Path, output: Output) -> Result<Exit, CliError> {
        match self {
            Self::List(args) => {
                let request = ThreadListRequest {
                    filters: args.into_filters(),
                };
                let page = with_archive(Archive::open_read_only(path), async |archive| {
                    list_threads(archive, &request).await
                })
                .await?;
                Ok(render_thread_page(output, &page))
            }
            Self::Show { reference } => {
                let detail = with_archive(Archive::open_read_only(path), async |archive| {
                    show_thread(archive, &reference).await
                })
                .await?;
                Ok(render_thread_detail(output, &detail))
            }
        }
    }
}
