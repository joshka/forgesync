//! Resolve GitHub clients for acquisition commands.

use std::collections::HashMap;

use forgesync_core::identity::GitHubHost;
use forgesync_engine::reference::RepositorySelector;
use forgesync_github::transport::{GitHubClient, GitHubClientConfig};
use tokio_util::sync::CancellationToken;

use crate::credentials::{CredentialError, resolve_token};
use crate::error::CliError;

/// Builds one provider client per selected host, discovering each host's credential once.
///
/// Unavailable optional credentials fall back to anonymous access; malformed credentials and
/// cancellation fail setup. No GitHub acquisition request is sent.
pub async fn github_clients_for_selectors(
    selectors: &[RepositorySelector],
    verbose: u8,
    cancellation: &CancellationToken,
) -> Result<HashMap<GitHubHost, GitHubClient>, CliError> {
    let mut hosts = selectors
        .iter()
        .map(|selector| selector.host().clone())
        .collect::<Vec<_>>();
    hosts.sort();
    hosts.dedup();

    let mut clients = HashMap::with_capacity(hosts.len());
    for host in hosts {
        let token = match resolve_token(&host, cancellation).await {
            Ok(token) => Some(token),
            Err(
                CredentialError::NoCredential
                | CredentialError::CommandUnavailable
                | CredentialError::CommandFailed
                | CredentialError::TimedOut,
            ) => {
                if verbose > 0 {
                    eprintln!("forgesync: no usable GitHub token for {host}; trying anonymously");
                }
                None
            }
            Err(CredentialError::Cancelled) => return Err(CliError::SetupCancelled),
            Err(error) => return Err(CliError::Credential(error)),
        };
        let base_url =
            url::Url::parse(&github_api_base_url(&host)).map_err(CliError::GitHubApiUrl)?;
        let client = GitHubClient::new(GitHubClientConfig::new(base_url), token)
            .map_err(CliError::GitHubClient)?;
        clients.insert(host, client);
    }
    Ok(clients)
}

/// Public GitHub uses its API host; Enterprise hosts use the REST `/api/v3/` prefix.
fn github_api_base_url(host: &GitHubHost) -> String {
    if host.as_str() == "github.com" {
        "https://api.github.com/".to_owned()
    } else {
        format!("{}/api/v3/", host.https_origin())
    }
}
