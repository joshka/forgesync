//! # Inspect run history and failed jobs
//!
//! `RunCommand` owns list and show requests for the durable workflow ledger. Its run method opens
//! an existing archive, asks the engine for run projections, and chooses human or JSON output.
//!
//! Run records explain attempts and partial failures; discussion coverage explains acquired source
//! evidence. Keeping both concepts visible helps users decide whether to inspect, retry, or
//! refresh.

use std::path::Path;
use std::process::ExitCode;

use clap::Subcommand;
use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::RunId;
use forgesync_engine::runs::{list_runs, show_run};
use forgesync_store::archive::Archive;

use super::RunFamilyArg;
use super::retry::execute_retry;
use crate::reports::{run_detail_summary, run_list_summary};
use crate::{OutputMode, render_engine_error, render_store_error, render_success, usage_error};

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
        id: u64,
    },
    /// Retry unresolved failures from one run.
    Retry {
        /// Positive archive-local run ID.
        id: u64,
        /// Limit retries to selected evidence families; repeat or comma-separate values.
        #[arg(long, value_enum, value_delimiter = ',')]
        family: Vec<RunFamilyArg>,
    },
}

impl RunCommand {
    /// Runs a local read or retries the selected unresolved work.
    pub async fn run(self, path: &Path, output: OutputMode, verbose: u8) -> ExitCode {
        match self {
            Self::List { limit } => Self::list(path, output, limit).await,
            Self::Show { id } => Self::show(path, output, id).await,
            Self::Retry { id, family } => Self::retry(path, output, verbose, id, family).await,
        }
    }

    /// Reads recent durable runs without changing the archive.
    async fn list(path: &Path, output: OutputMode, limit: u32) -> ExitCode {
        let archive = match Archive::open_read_only(path).await {
            Ok(archive) => archive,
            Err(error) => return render_store_error(output, "run list", error),
        };
        let result = list_runs(&archive, limit).await;
        archive.close().await;
        match result {
            Ok(runs) => render_success(output, "run list", &runs, run_list_summary),
            Err(error) => render_engine_error(output, "run list", error),
        }
    }

    /// Reads a run's job and failure ledger after validating its archive-local ID.
    async fn show(path: &Path, output: OutputMode, id: u64) -> ExitCode {
        let id = match RunId::new(id) {
            Ok(id) => id,
            Err(_) => return usage_error("run ID must be a positive integer"),
        };
        let archive = match Archive::open_read_only(path).await {
            Ok(archive) => archive,
            Err(error) => return render_store_error(output, "run show", error),
        };
        let result = show_run(&archive, id).await;
        archive.close().await;
        match result {
            Ok(detail) => render_success(output, "run show", &detail, run_detail_summary),
            Err(error) => render_engine_error(output, "run show", error),
        }
    }

    /// Starts cancellation-aware acquisition for unresolved family failures.
    async fn retry(
        path: &Path,
        output: OutputMode,
        verbose: u8,
        id: u64,
        family: Vec<RunFamilyArg>,
    ) -> ExitCode {
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
        let families = family.into_iter().map(EvidenceFamily::from).collect();
        let result = execute_retry(path, id, families, output, verbose, &cancellation).await;
        interrupt_task.abort();
        result
    }
}
