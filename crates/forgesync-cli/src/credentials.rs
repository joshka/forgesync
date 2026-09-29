//! # Resolve GitHub authentication for the CLI
//!
//! `GitHubCredentialSettings` controls how the process finds a token, and `resolve_github_token`
//! returns the selected credential or a typed `CredentialError`. Acquisition commands use this
//! before constructing provider clients.
//!
//! Credential lookup belongs here because it is a process concern. The GitHub adapter receives a
//! token value but does not choose environment variables or print secrets; diagnostics should
//! describe the missing source without exposing the credential.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use forgesync_core::identity::GitHubHost;
use forgesync_github::token::GitHubToken;
use thiserror::Error;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

/// Credential lookup order and bounded `gh` fallback settings.
#[derive(Clone, Debug)]
pub struct GitHubCredentialSettings {
    /// Optional environment variable to check before `GITHUB_TOKEN`.
    pub configured_token_environment_variable: Option<String>,
    /// Program used for host-aware `gh auth token` discovery.
    pub gh_program: PathBuf,
    /// Maximum time to wait for credential discovery.
    pub command_timeout: Duration,
}

impl Default for GitHubCredentialSettings {
    fn default() -> Self {
        Self {
            configured_token_environment_variable: None,
            gh_program: PathBuf::from("gh"),
            command_timeout: Duration::from_secs(5),
        }
    }
}

/// Credential discovery failures that do not reveal token values or subprocess output.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CredentialError {
    /// A configured environment variable name is invalid.
    #[error("configured GitHub token environment variable name is invalid")]
    InvalidEnvironmentVariable,
    /// A non-empty token value contains invalid characters.
    #[error("GitHub token environment variable contains an invalid value")]
    InvalidToken,
    /// Neither supported environment variables nor `gh` provided a token.
    #[error("no GitHub credential is available")]
    NoCredential,
    /// The `gh` executable could not be started.
    #[error("GitHub CLI credential helper could not be started")]
    CommandUnavailable,
    /// The `gh` command exited unsuccessfully.
    #[error("GitHub CLI credential helper failed")]
    CommandFailed,
    /// Credential discovery exceeded its timeout.
    #[error("GitHub CLI credential lookup timed out")]
    TimedOut,
    /// Credential discovery was cancelled by its caller.
    #[error("GitHub credential lookup was cancelled")]
    Cancelled,
}

/// Resolves a GitHub token from configured environment, `GITHUB_TOKEN`, then `gh`.
pub async fn resolve_github_token(
    settings: &GitHubCredentialSettings,
    host: &GitHubHost,
    cancellation: &CancellationToken,
) -> Result<GitHubToken, CredentialError> {
    if let Some(name) = settings.configured_token_environment_variable.as_deref()
        && !valid_environment_variable_name(name)
    {
        return Err(CredentialError::InvalidEnvironmentVariable);
    }

    let configured = settings
        .configured_token_environment_variable
        .as_deref()
        .and_then(std::env::var_os);
    let github_token = std::env::var_os("GITHUB_TOKEN");
    if let Some(token) = choose_environment_token(configured, github_token)? {
        return Ok(token);
    }
    if settings.command_timeout.is_zero() {
        return Err(CredentialError::TimedOut);
    }

    let arguments = [
        OsString::from("auth"),
        OsString::from("token"),
        OsString::from("--hostname"),
        OsString::from(host.as_str()),
    ];
    let output = run_credential_process(
        &settings.gh_program,
        &arguments,
        settings.command_timeout,
        cancellation,
    )
    .await?;
    let token = std::str::from_utf8(&output.stdout).map_err(|_| CredentialError::InvalidToken)?;
    let token = token.trim();
    if token.is_empty() {
        return Err(CredentialError::NoCredential);
    }
    GitHubToken::new(token).map_err(|_| CredentialError::InvalidToken)
}

/// Uses the configured token before `GITHUB_TOKEN`, ignoring empty values but rejecting a
/// non-Unicode or malformed nonempty value instead of silently trying a lower-priority source.
fn choose_environment_token(
    configured: Option<OsString>,
    github_token: Option<OsString>,
) -> Result<Option<GitHubToken>, CredentialError> {
    configured
        .into_iter()
        .chain(github_token)
        .find_map(|value| match value.into_string() {
            Ok(value) if value.trim().is_empty() => None,
            Ok(value) => {
                Some(GitHubToken::new(value.trim()).map_err(|_| CredentialError::InvalidToken))
            }
            Err(_) => Some(Err(CredentialError::InvalidToken)),
        })
        .transpose()
}

/// Accepts only variable names that can be read consistently across supported shells.
pub(crate) fn valid_environment_variable_name(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

/// Runs the credential helper without stdin or visible stderr. Cancellation and timeout both
/// drop the child with kill-on-drop enabled, so credential lookup cannot block shutdown.
async fn run_credential_process(
    program: &Path,
    arguments: &[OsString],
    timeout: Duration,
    cancellation: &CancellationToken,
) -> Result<std::process::Output, CredentialError> {
    if cancellation.is_cancelled() {
        return Err(CredentialError::Cancelled);
    }
    let mut command = Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let child = command
        .spawn()
        .map_err(|_| CredentialError::CommandUnavailable)?;

    let output = tokio::select! {
        _ = cancellation.cancelled() => return Err(CredentialError::Cancelled),
        result = tokio::time::timeout(timeout, child.wait_with_output()) => {
            match result {
                Ok(result) => result.map_err(|_| CredentialError::CommandFailed)?,
                Err(_) => return Err(CredentialError::TimedOut),
            }
        }
    };
    if !output.status.success() {
        return Err(CredentialError::CommandFailed);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;
    use std::time::Duration;

    use tokio_util::sync::CancellationToken;

    use super::{
        CredentialError, choose_environment_token, run_credential_process,
        valid_environment_variable_name,
    };

    #[test]
    fn configured_environment_token_precedes_github_token() {
        let selected = choose_environment_token(
            Some(OsString::from("configured-token")),
            Some(OsString::from("github-token")),
        )
        .expect("valid tokens");

        assert_eq!(selected.expect("token").expose(), "configured-token");
    }

    #[test]
    fn empty_configured_environment_token_falls_back_to_github_token() {
        let selected = choose_environment_token(
            Some(OsString::from("  ")),
            Some(OsString::from("github-token")),
        )
        .expect("valid token");

        assert_eq!(selected.expect("token").expose(), "github-token");
    }

    #[test]
    fn environment_variable_names_are_checked_before_lookup() {
        assert!(valid_environment_variable_name("FORGESYNC_GITHUB_TOKEN"));
        assert!(!valid_environment_variable_name("9TOKEN"));
        assert!(!valid_environment_variable_name("TOKEN;echo"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn credential_subprocess_timeout_is_bounded() {
        let result = run_credential_process(
            Path::new("/bin/sleep"),
            &[OsString::from("5")],
            Duration::from_millis(30),
            &CancellationToken::new(),
        )
        .await;

        assert_eq!(result, Err(CredentialError::TimedOut));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn credential_subprocess_cancellation_is_bounded() {
        let cancellation = CancellationToken::new();
        let child_cancellation = cancellation.clone();
        let task = tokio::spawn(async move {
            run_credential_process(
                Path::new("/bin/sleep"),
                &[OsString::from("5")],
                Duration::from_secs(10),
                &child_cancellation,
            )
            .await
        });
        tokio::time::sleep(Duration::from_millis(30)).await;
        cancellation.cancel();

        assert_eq!(
            task.await.expect("credential task"),
            Err(CredentialError::Cancelled)
        );
    }
}
