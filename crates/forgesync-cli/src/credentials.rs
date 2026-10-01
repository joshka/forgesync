//! Resolve GitHub authentication for the CLI.
//!
//! Lookup checks `GITHUB_TOKEN`, then runs `gh auth token --hostname <host>`. A whitespace-only
//! environment value falls back to `gh`; a malformed nonempty value fails lookup instead of hiding
//! a broken high-priority source. Errors carry classifications, never token values or helper
//! output. Callers decide whether an unavailable credential permits anonymous acquisition.

use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use forgesync_core::identity::GitHubHost;
use forgesync_github::token::GitHubToken;
use thiserror::Error;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

/// Maximum wait for the `gh` helper after it starts.
const GH_TIMEOUT: Duration = Duration::from_secs(5);

/// Credential discovery failures that do not reveal token values or subprocess output.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CredentialError {
    #[error("GitHub token environment variable contains an invalid value")]
    InvalidToken,
    #[error("no GitHub credential is available")]
    NoCredential,
    #[error("GitHub CLI credential helper could not be started")]
    CommandUnavailable,
    #[error("GitHub CLI credential helper failed")]
    CommandFailed,
    #[error("GitHub CLI credential lookup timed out")]
    TimedOut,
    #[error("GitHub credential lookup was cancelled")]
    Cancelled,
}

/// Resolves a checked token from the environment, then from `gh` for the selected host.
///
/// An environment token is returned without starting a process, even when cancellation is set.
/// Cancellation and the timeout govern only the helper.
pub async fn resolve_token(
    host: &GitHubHost,
    cancellation: &CancellationToken,
) -> Result<GitHubToken, CredentialError> {
    if let Some(token) = environment_token(std::env::var_os("GITHUB_TOKEN"))? {
        return Ok(token);
    }
    let arguments = [
        OsString::from("auth"),
        OsString::from("token"),
        OsString::from("--hostname"),
        OsString::from(host.as_str()),
    ];
    let output =
        run_credential_process(Path::new("gh"), &arguments, GH_TIMEOUT, cancellation).await?;
    let token = std::str::from_utf8(&output.stdout).map_err(|_| CredentialError::InvalidToken)?;
    let token = token.trim();
    if token.is_empty() {
        return Err(CredentialError::NoCredential);
    }
    GitHubToken::new(token).map_err(|_| CredentialError::InvalidToken)
}

/// Ignores an empty value but rejects a non-Unicode or malformed nonempty one.
fn environment_token(value: Option<OsString>) -> Result<Option<GitHubToken>, CredentialError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value
        .into_string()
        .map_err(|_| CredentialError::InvalidToken)?;
    if value.trim().is_empty() {
        return Ok(None);
    }
    GitHubToken::new(value.trim())
        .map(Some)
        .map_err(|_| CredentialError::InvalidToken)
}

/// Accepts only variable names that can be read consistently across supported shells.
pub fn valid_environment_variable_name(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

/// Runs a helper directly (no shell) with no stdin, captured stdout, and discarded stderr.
///
/// The timeout covers waiting after spawn. Cancellation or timeout drops the child with
/// kill-on-drop enabled. Only a successful exit returns captured output.
async fn run_credential_process(
    program: &Path,
    arguments: &[OsString],
    timeout: Duration,
    cancellation: &CancellationToken,
) -> Result<std::process::Output, CredentialError> {
    if cancellation.is_cancelled() {
        return Err(CredentialError::Cancelled);
    }
    let child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
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
mod tests;
