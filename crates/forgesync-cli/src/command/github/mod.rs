//! # Resolve GitHub clients for acquisition commands
//!
//! Client setup combines selected hosts, credential settings, and provider endpoint rules into
//! typed GitHub adapters. Errors here are presented as configuration or authentication failures
//! before a sync workflow begins.
//!
//! The adapter itself owns HTTP and normalization. This module owns process configuration and
//! token resolution, so engine code does not read environment variables or local config files.
//!
//! Selected hosts are sorted and deduplicated before credential lookup, avoiding repeated helper
//! invocations for repositories on the same host. Unavailable optional credentials allow anonymous
//! acquisition; malformed configured credentials fail setup, and cancellation remains distinct.
//! [`GitHubClientSetupError`] retains safe typed causes until commands close resources and render
//! their stable process envelope.

use std::collections::HashMap;
use std::process::ExitCode;

use forgesync_core::identity::GitHubHost;
use forgesync_engine::reference::RepositorySelector;
use forgesync_github::error::GitHubError;
use forgesync_github::transport::{GitHubClient, GitHubClientConfig};

use crate::credentials::CredentialError;
use crate::{OutputMode, render_error_with_status};

/// Builds one provider client per selected host, using process-level credential discovery.
///
/// Host selectors are deduplicated before lookup. Missing, unavailable, failed, or timed-out
/// optional credentials permit anonymous clients; malformed configured credentials return a typed
/// setup error. Cancellation interrupts discovery before acquisition starts. This operation may
/// read process credential settings and invoke the `gh` helper, but sends no GitHub acquisition
/// requests.
///
/// # Errors
///
/// Returns [`GitHubClientSetupError`] for invalid credentials, cancellation, an invalid derived API
/// URL, or adapter initialization failure. The caller owns archive cleanup before rendering it.
pub async fn github_clients_for_selectors(
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

    let settings = crate::credentials::GitHubCredentialSettings::default();
    let mut clients = HashMap::with_capacity(hosts.len());
    for host in hosts {
        let token = match settings.resolve_token(&host, cancellation).await {
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
            Err(error) => return Err(GitHubClientSetupError::Credential(error)),
        };
        let base_url = github_api_base_url(&host);
        let config = match url::Url::parse(&base_url) {
            Ok(url) => GitHubClientConfig::new(url),
            Err(error) => return Err(GitHubClientSetupError::InvalidApiUrl(error)),
        };
        match GitHubClient::new(config, token) {
            Ok(client) => {
                clients.insert(host, client);
            }
            Err(error) => return Err(GitHubClientSetupError::Initialization(error)),
        }
    }
    Ok(clients)
}

/// Failure preparing a host client before any GitHub acquisition begins.
///
/// Credential, URL, and adapter causes remain typed through command cleanup. Their safe display
/// messages become CLI envelopes only at rendering; callers can also inspect the error source.
/// Cancellation stays distinct so sync, refresh, and retry can preserve their interruption status.
#[derive(Debug, thiserror::Error)]
pub enum GitHubClientSetupError {
    /// Credential discovery was interrupted before a provider client could be prepared.
    #[error("operation was cancelled before GitHub acquisition began")]
    Cancelled,
    /// A configured credential source contained invalid settings or token data.
    #[error("{0}")]
    Credential(#[source] CredentialError),
    /// A host's derived API endpoint could not be parsed; no request was sent.
    #[error("could not build GitHub API URL")]
    InvalidApiUrl(#[source] url::ParseError),
    /// The typed provider adapter rejected its configuration or could not initialize transport.
    #[error("{0}")]
    Initialization(#[source] GitHubError),
}

/// Converts provider client setup failures into the CLI's stable error envelope.
///
/// JSON errors go to stdout; human diagnostics go to stderr. Cancellation returns status 130;
/// other setup failures return the failure status. Error codes describe the failed boundary rather
/// than depending on the cause's diagnostic wording. This renderer owns no archive or task: callers
/// close their resources before handing it a retained setup failure.
pub fn render_github_client_setup_error(
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
        GitHubClientSetupError::Credential(error) => (
            "github_credential_invalid",
            error.to_string(),
            ExitCode::FAILURE,
        ),
        GitHubClientSetupError::InvalidApiUrl(_) => (
            "github_api_url_invalid",
            "could not build GitHub API URL".to_owned(),
            ExitCode::FAILURE,
        ),
        GitHubClientSetupError::Initialization(error) => (
            "github_client_initialization_failed",
            error.to_string(),
            ExitCode::FAILURE,
        ),
    };
    render_error_with_status(json, command, code, &message, status)
}

/// Returns the API base URL corresponding to a validated GitHub host.
///
/// Public GitHub uses its dedicated API host. Enterprise hosts retain their HTTPS origin and use
/// the REST `/api/v3/` prefix. Construction performs no request or credential lookup.
pub fn github_api_base_url(host: &GitHubHost) -> String {
    if host.as_str() == "github.com" {
        "https://api.github.com/".to_owned()
    } else {
        format!("{}/api/v3/", host.https_origin())
    }
}

#[cfg(test)]
mod tests;
