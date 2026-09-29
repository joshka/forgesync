//! Github command handling.

use super::*;

pub(super) async fn github_clients_for_selectors(
    selectors: &[RepositorySelector],
    verbose: u8,
    cancellation: &tokio_util::sync::CancellationToken,
) -> Result<HashMap<GitHubHost, GitHubClient>, GitHubClientSetupError> {
    let mut hosts = selectors
        .iter()
        .map(|selector| selector.host().clone())
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
                return Err(GitHubClientSetupError::Cancelled);
            }
            Err(error) => return Err(GitHubClientSetupError::Credential(error.to_string())),
        };
        let base_url = github_api_base_url(&host);
        let config = match url::Url::parse(&base_url) {
            Ok(url) => GitHubClientConfig::new(url),
            Err(_) => return Err(GitHubClientSetupError::InvalidApiUrl),
        };
        match GitHubClient::new(config, token) {
            Ok(client) => {
                clients.insert(host, client);
            }
            Err(error) => return Err(GitHubClientSetupError::Initialization(error.to_string())),
        }
    }
    Ok(clients)
}

#[derive(Debug)]
pub(super) enum GitHubClientSetupError {
    Cancelled,
    Credential(String),
    InvalidApiUrl,
    Initialization(String),
}

pub(super) fn render_github_client_setup_error(
    json: OutputMode,
    command: &str,
    error: GitHubClientSetupError,
) -> ExitCode {
    let (code, message, status) = match error {
        GitHubClientSetupError::Cancelled => (
            "operation_cancelled",
            "operation was cancelled before GitHub acquisition began".to_owned(),
            ExitCode::from(130),
        ),
        GitHubClientSetupError::Credential(message) => {
            ("github_credential_invalid", message, ExitCode::FAILURE)
        }
        GitHubClientSetupError::InvalidApiUrl => (
            "github_api_url_invalid",
            "could not build GitHub API URL".to_owned(),
            ExitCode::FAILURE,
        ),
        GitHubClientSetupError::Initialization(message) => (
            "github_client_initialization_failed",
            message,
            ExitCode::FAILURE,
        ),
    };
    render_error_with_status(json, command, code, &message, status)
}

pub(super) fn github_api_base_url(host: &GitHubHost) -> String {
    if host.as_str() == "github.com" {
        "https://api.github.com/".to_owned()
    } else {
        format!("{}/api/v3/", host.https_origin())
    }
}
