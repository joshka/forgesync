//! Retry command handling.

use std::collections::HashMap;
use std::process::ExitCode;

use forgesync_core::identity::RunId;
use forgesync_engine::runs::{plan_run_retry, run_retry};
use forgesync_engine::sync::SyncProgress;
use forgesync_github::transport::{GitHubClient, GitHubClientConfig};
use forgesync_store::archive::Archive;

use super::github::github_api_base_url;
use crate::reports::{SyncFailure, outcome_exit_code, retry_summary};
use crate::{OutputMode, render_engine_error, render_error, render_result, render_store_error};

pub async fn retry_command(
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
    let mut hosts = plan
        .scopes
        .iter()
        .map(|scope| scope.repository.host().clone())
        .collect::<Vec<_>>();
    hosts.sort();
    hosts.dedup();

    let mut clients = HashMap::with_capacity(hosts.len());
    for host in hosts {
        let token = match crate::credentials::resolve_github_token(
            &crate::credentials::GitHubCredentialSettings::default(),
            &host,
            cancellation,
        )
        .await
        {
            Ok(token) => Some(token),
            Err(
                crate::credentials::CredentialError::NoCredential
                | crate::credentials::CredentialError::CommandUnavailable
                | crate::credentials::CredentialError::CommandFailed
                | crate::credentials::CredentialError::TimedOut,
            ) => {
                if verbose > 0 {
                    eprintln!("forgesync: no usable GitHub token for {host}; trying anonymously");
                }
                None
            }
            Err(crate::credentials::CredentialError::Cancelled) => {
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
                return render_error(
                    json,
                    "run retry",
                    "github_credential_invalid",
                    &error.to_string(),
                );
            }
        };
        let config = match url::Url::parse(&github_api_base_url(&host)) {
            Ok(url) => GitHubClientConfig::new(url),
            Err(_) => {
                archive.close().await;
                return render_error(
                    json,
                    "run retry",
                    "github_api_url_invalid",
                    "could not build GitHub API URL",
                );
            }
        };
        match GitHubClient::new(config, token) {
            Ok(client) => {
                clients.insert(host, client);
            }
            Err(error) => {
                archive.close().await;
                return render_error(
                    json,
                    "run retry",
                    "github_client_initialization_failed",
                    &error.to_string(),
                );
            }
        }
    }

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
