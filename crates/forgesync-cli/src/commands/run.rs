//! Run command handling.

use std::process::ExitCode;

use forgesync_core::identity::RunId;
use forgesync_engine::runs::{list_runs, show_run};
use forgesync_store::archive::Archive;

use super::retry::retry_command;
use crate::args::{RunCommand, RunFamilyArg};
use crate::reports::{run_detail_summary, run_list_summary};
use crate::{OutputMode, render_engine_error, render_store_error, render_success, usage_error};

pub async fn run_command(
    path: &std::path::Path,
    json: OutputMode,
    verbose: u8,
    command: RunCommand,
) -> ExitCode {
    match command {
        RunCommand::List { limit } => match Archive::open_read_only(path).await {
            Ok(archive) => {
                let result = list_runs(&archive, limit).await;
                archive.close().await;
                match result {
                    Ok(runs) => render_success(json, "run list", &runs, run_list_summary),
                    Err(error) => render_engine_error(json, "run list", error),
                }
            }
            Err(error) => render_store_error(json, "run list", error),
        },
        RunCommand::Show { id } => {
            let id = match RunId::new(id) {
                Ok(id) => id,
                Err(_) => return usage_error("run ID must be a positive integer"),
            };
            match Archive::open_read_only(path).await {
                Ok(archive) => {
                    let result = show_run(&archive, id).await;
                    archive.close().await;
                    match result {
                        Ok(detail) => render_success(json, "run show", &detail, run_detail_summary),
                        Err(error) => render_engine_error(json, "run show", error),
                    }
                }
                Err(error) => render_store_error(json, "run show", error),
            }
        }
        RunCommand::Retry { id, family } => {
            let id = match RunId::new(id) {
                Ok(id) => id,
                Err(_) => return usage_error("run ID must be a positive integer"),
            };
            let cancellation = tokio_util::sync::CancellationToken::new();
            let interrupt_cancellation = cancellation.clone();
            let interrupt_task = tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    interrupt_cancellation.cancel();
                }
            });
            let result = retry_command(
                path,
                id,
                family
                    .into_iter()
                    .map(|family| match family {
                        RunFamilyArg::Threads => forgesync_core::coverage::EvidenceFamily::Threads,
                        RunFamilyArg::Comments => {
                            forgesync_core::coverage::EvidenceFamily::Comments
                        }
                        RunFamilyArg::PullRequestMetadata => {
                            forgesync_core::coverage::EvidenceFamily::PullRequestMetadata
                        }
                        RunFamilyArg::Reviews => forgesync_core::coverage::EvidenceFamily::Reviews,
                        RunFamilyArg::ReviewThreads => {
                            forgesync_core::coverage::EvidenceFamily::ReviewThreads
                        }
                    })
                    .collect(),
                json,
                verbose,
                &cancellation,
            )
            .await;
            interrupt_task.abort();
            result
        }
    }
}
