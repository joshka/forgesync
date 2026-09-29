//! # Retry work recorded as failed
//!
//! The retry handler selects a run or failure scope, builds an engine retry plan, and executes the
//! requested work. It reports a new outcome while leaving earlier run history inspectable.
//!
//! Retry is based on durable failure records and current archive state. It should not infer work
//! merely from absent content, because a family may never have been requested.

use std::process::ExitCode;

use forgesync_core::identity::RunId;
use forgesync_engine::runs::{plan_run_retry, run_retry};
use forgesync_engine::sync::SyncProgress;
use forgesync_store::archive::Archive;

use super::github::{
    GitHubClientSetupError, github_clients_for_selectors, render_github_client_setup_error,
};
use crate::reports::{SyncFailure, outcome_exit_code, retry_summary};
use crate::{OutputMode, render_engine_error, render_result, render_store_error};

/// Plans unresolved work, constructs host clients, and retries selected run families.
///
/// The initial plan is read from the archive before provider clients are constructed. A partial
/// retry remains a durable run report, with its own exit status and failure details.
pub async fn execute_retry(
    archive_path: &std::path::Path,
    run_id: RunId,
    families: Vec<forgesync_core::coverage::EvidenceFamily>,
    json: OutputMode,
    verbose: u8,
    cancellation: &tokio_util::sync::CancellationToken,
) -> ExitCode {
    let archive = match Archive::open_read_write(archive_path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(json, "run retry", error),
    };
    let plan = match plan_run_retry(&archive, run_id, &families).await {
        Ok(plan) => plan,
        Err(error) => {
            archive.close().await;
            return render_engine_error(json, "run retry", error);
        }
    };
    let repositories = plan
        .scopes
        .iter()
        .map(|scope| scope.repository.clone())
        .collect::<Vec<_>>();
    let clients = match github_clients_for_selectors(&repositories, verbose, cancellation).await {
        Ok(clients) => clients,
        Err(GitHubClientSetupError::Cancelled) => {
            archive.close().await;
            return render_result(
                json,
                "run retry",
                &SyncFailure {
                    code: "operation_cancelled",
                    message: "retry was cancelled before acquisition began".to_owned(),
                },
                |failure| failure.message.clone(),
                ExitCode::from(130),
            );
        }
        Err(error) => {
            archive.close().await;
            return render_github_client_setup_error(json, "run retry", error);
        }
    };

    let (progress_sender, mut progress_receiver) = tokio::sync::mpsc::channel::<SyncProgress>(4);
    let progress_task = if verbose > 0 && !json.is_json() {
        Some(tokio::spawn(async move {
            while let Some(progress) = progress_receiver.recv().await {
                let repository = progress.repository.as_deref().unwrap_or("retry");
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
    let result = run_retry(
        &archive,
        &clients,
        plan,
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
            let exit_status = report
                .runs
                .iter()
                .map(|run| outcome_exit_code(&run.outcome))
                .find(|status| *status != ExitCode::SUCCESS)
                .unwrap_or(ExitCode::SUCCESS);
            render_result(json, "run retry", &report, retry_summary, exit_status)
        }
        Err(error) => render_engine_error(json, "run retry", error),
    }
}
