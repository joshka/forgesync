//! # Retry work recorded as failed
//!
//! [`RetryRequest`] selects a durable run and optional evidence families. Its run method opens the
//! writable archive, obtains a retry plan, constructs only the required host clients, and executes
//! the plan. A new report preserves prior run history and may describe partial success.
//!
//! Retry is based on unresolved failure records and current archive state. It does not infer work
//! merely from absent content, because a family may never have been requested. Planning therefore
//! precedes credential discovery and provider acquisition.
//!
//! The outer run method owns archive closure for both planning and acquisition failures.
//! [`RetryFailure`] retains the failed boundary until rendering, including the distinct
//! cancellation status before acquisition. [`ProgressReporter`] supplies advisory stderr events;
//! final exit status comes from the retry report's ordered run outcomes.

use std::path::Path;
use std::process::ExitCode;

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::RunId;
use forgesync_engine::error::EngineError;
use forgesync_engine::runs::{RetryReport, plan_run_retry, run_retry};
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;

use super::github::{
    GitHubClientSetupError, github_clients_for_selectors, render_github_client_setup_error,
};
use super::progress::ProgressReporter;
use crate::reports::runs::{RetryCancellation, retry_summary};
use crate::reports::sync::outcome_exit_code;
use crate::{OutputMode, render_engine_error, render_result, render_store_error};

/// Exact durable failure selection for one CLI retry invocation.
///
/// Process output and cancellation are supplied separately so they cannot alter which failure
/// records the engine selects. Empty `families` selects every unresolved family on the run.
pub struct RetryRequest {
    /// Archive-local run whose unresolved failures are eligible for retry.
    pub run_id: RunId,
    /// Optional restriction on evidence families; empty selects all unresolved families.
    pub families: Vec<EvidenceFamily>,
}

impl RetryRequest {
    /// Executes selected work and closes the archive before rendering any terminal outcome.
    ///
    /// Path, output, verbosity, and cancellation are process context rather than retry scope.
    /// Planning happens before client preparation; partial acquisition remains a durable report.
    pub async fn run(
        self,
        path: &Path,
        output: OutputMode,
        verbose: u8,
        cancellation: &CancellationToken,
    ) -> ExitCode {
        let archive = match Archive::open_read_write(path).await {
            Ok(archive) => archive,
            Err(error) => return render_store_error(output, "run retry", error),
        };
        let result = self.acquire(&archive, output, verbose, cancellation).await;
        archive.close().await;
        match result {
            Ok(report) => render_report(output, report),
            Err(error) => error.render(output),
        }
    }

    /// Plans locally, prepares selected hosts, and drains progress after acquisition finishes.
    async fn acquire(
        self,
        archive: &Archive,
        output: OutputMode,
        verbose: u8,
        cancellation: &CancellationToken,
    ) -> Result<RetryReport, RetryFailure> {
        let progress = ProgressReporter::start("retry", output, verbose);
        let plan = plan_run_retry(archive, self.run_id, &self.families)
            .await
            .map_err(RetryFailure::Engine)?;
        let repositories = plan
            .scopes
            .iter()
            .map(|scope| scope.repository.clone())
            .collect::<Vec<_>>();
        let clients = github_clients_for_selectors(&repositories, verbose, cancellation)
            .await
            .map_err(RetryFailure::ClientSetup)?;
        let result = run_retry(archive, &clients, plan, cancellation, progress.sender()).await;
        progress.finish().await;
        result.map_err(RetryFailure::Engine)
    }
}

/// Renders the report using the first non-success run status in engine execution order.
fn render_report(output: OutputMode, report: RetryReport) -> ExitCode {
    let exit_status = report
        .runs
        .iter()
        .map(|run| outcome_exit_code(&run.outcome))
        .find(|status| *status != ExitCode::SUCCESS)
        .unwrap_or(ExitCode::SUCCESS);
    render_result(output, "run retry", &report, retry_summary, exit_status)
}

/// Failure boundary retained until the command has closed its archive.
#[derive(Debug, thiserror::Error)]
enum RetryFailure {
    /// Local planning or acquisition could not produce a retry report.
    #[error("acquisition failed: {0}")]
    Engine(#[source] EngineError),
    /// Host credential discovery or provider transport initialization failed before acquisition.
    #[error("provider client setup failed: {0}")]
    ClientSetup(#[source] GitHubClientSetupError),
}

impl RetryFailure {
    /// Preserves engine and pre-acquisition setup error envelopes after resource cleanup.
    fn render(self, output: OutputMode) -> ExitCode {
        match self {
            Self::Engine(error) => render_engine_error(output, "run retry", error),
            Self::ClientSetup(error) => render_setup_error(output, error),
        }
    }
}

/// Distinguishes cancellation during credential discovery from other setup failures.
fn render_setup_error(output: OutputMode, error: GitHubClientSetupError) -> ExitCode {
    match error {
        GitHubClientSetupError::Cancelled => render_cancelled(output),
        error => render_github_client_setup_error(output, "run retry", error),
    }
}

/// Reports interruption before provider acquisition using the command's established status 130.
fn render_cancelled(output: OutputMode) -> ExitCode {
    let failure = RetryCancellation {
        code: "operation_cancelled",
        message: "retry was cancelled before acquisition began".to_owned(),
    };
    render_result(
        output,
        "run retry",
        &failure,
        |failure| failure.message.clone(),
        ExitCode::from(130),
    )
}
