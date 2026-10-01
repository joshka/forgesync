//! Inspect durable run history and retry unresolved work.
//!
//! Retry is driven by unresolved failure records, never by absent content, because a family may
//! never have been requested. Planning therefore precedes credential discovery and acquisition.

use std::path::Path;

use clap::Subcommand;
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::RunId;
use forgesync_engine::runs::{list_runs, plan_run_retry, run_retry, show_run};
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;

use super::github::github_clients_for_selectors;
use super::progress::ProgressReporter;
use super::with_archive;
use crate::command::values::RunFamilyArg;
use crate::error::{CliError, Exit};
use crate::output::Output;
use crate::reports::run_detail::run_detail_summary;
use crate::reports::runs::{retry_summary, run_list_summary};
use crate::reports::sync::outcome_exit_code;

/// Durable sync-run operations.
#[derive(Clone, Debug, Subcommand)]
pub enum RunCommand {
    /// List recent archive runs.
    List {
        /// Maximum number of recent runs to show (1-1000).
        #[arg(
            long,
            default_value_t = 50,
            value_parser = clap::value_parser!(u32).range(1..=1000)
        )]
        limit: u32,
    },
    /// Show one run with its jobs and failure ledger.
    Show {
        /// Positive archive-local run ID.
        #[arg(value_parser = parse_run_id)]
        id: RunId,
    },
    /// Retry unresolved failures from one run.
    Retry {
        /// Positive archive-local run ID.
        #[arg(value_parser = parse_run_id)]
        id: RunId,
        /// Limit retries to selected evidence families; repeat or comma-separate values.
        #[arg(long, value_enum, value_delimiter = ',')]
        family: Vec<RunFamilyArg>,
    },
}

/// Parses a positive archive-local run ID.
fn parse_run_id(value: &str) -> Result<RunId, String> {
    value
        .parse()
        .ok()
        .and_then(|id| RunId::new(id).ok())
        .ok_or_else(|| "run ID must be a positive integer".to_owned())
}

impl RunCommand {
    /// Lists or shows runs read-only, or retries unresolved failures with GitHub acquisition.
    pub async fn run(
        self,
        path: &Path,
        output: Output,
        verbose: u8,
        cancellation: &CancellationToken,
    ) -> Result<Exit, CliError> {
        match self {
            Self::List { limit } => {
                let runs = with_archive(Archive::open_read_only(path), async |archive| {
                    list_runs(archive, limit).await
                })
                .await?;
                Ok(output.success(&runs, run_list_summary))
            }
            Self::Show { id } => {
                let detail = with_archive(Archive::open_read_only(path), async |archive| {
                    show_run(archive, id).await
                })
                .await?;
                Ok(output.success(&detail, run_detail_summary))
            }
            Self::Retry { id, family } => {
                let families = family
                    .into_iter()
                    .map(EvidenceFamily::from)
                    .collect::<Vec<_>>();
                let report = with_archive(Archive::open_read_write(path), async |archive| {
                    let progress = ProgressReporter::start("retry", output.mode, verbose);
                    let plan = plan_run_retry(archive, id, &families).await?;
                    let repositories = plan
                        .scopes
                        .iter()
                        .map(|scope| scope.repository.clone())
                        .collect::<Vec<_>>();
                    let clients =
                        github_clients_for_selectors(&repositories, verbose, cancellation).await?;
                    let result =
                        run_retry(archive, &clients, plan, cancellation, progress.sender()).await;
                    progress.finish().await;
                    Ok::<_, CliError>(result?)
                })
                .await?;
                // The first non-success child run, in engine execution order, sets the status.
                let exit = report
                    .runs
                    .iter()
                    .map(|run| outcome_exit_code(&run.outcome))
                    .find(|exit| *exit != Exit::Success)
                    .unwrap_or(Exit::Success);
                Ok(output.report(&report, retry_summary, exit))
            }
        }
    }
}
