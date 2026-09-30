//! # Resolve GitHub authentication for the CLI
//!
//! `GitHubCredentialSettings` controls how the process finds a token, and
//! [`GitHubCredentialSettings::resolve_token`] returns the selected credential or a typed
//! `CredentialError`. Acquisition commands use this before constructing provider clients.
//!
//! Credential lookup belongs here because it is a process concern. The GitHub adapter receives a
//! token value but does not choose environment variables or print secrets; diagnostics should
//! describe the missing source without exposing the credential.
//!
//! Lookup checks the configured environment variable, then `GITHUB_TOKEN`, then invokes the
//! configured `gh` executable with `auth token --hostname` for the selected host. Whitespace-only
//! environment values allow fallback; malformed nonempty values reject lookup instead of hiding a
//! broken high-priority source. Returned values pass through the adapter's checked token type.
//!
//! Environment selection is synchronous and precedes subprocess cancellation/timeout checks. A
//! token already available in environment can therefore be returned without starting a process,
//! even when cancellation is set. Only the helper path uses the configured command timeout.
//!
//! The child receives no stdin, captures token stdout, discards stderr, and is killed on drop when
//! lookup is cancelled or times out. Errors retain classifications rather than captured output.
//! Callers decide whether unavailable discovery permits anonymous acquisition; this module does
//! not silently convert malformed tokens or cancellation into anonymous requests.

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
    /// Maximum asynchronous helper wait after successful spawn; environment lookup is excluded.
    pub command_timeout: Duration,
}

impl Default for GitHubCredentialSettings {
    /// Configures standard environment discovery followed by a five-second host-aware `gh`
    /// fallback. Constructing these settings performs no lookup and selects no custom token
    /// variable.
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

impl GitHubCredentialSettings {
    /// Resolves a checked token using environment precedence followed by host-aware `gh`.
    ///
    /// Validates the configured variable name before reading process environment. Empty/whitespace
    /// values are ignored; malformed nonempty or non-Unicode values stop lookup. A selected token
    /// is trimmed but not verified against GitHub. The host affects helper arguments, not
    /// environment token selection: callers own choosing an environment credential appropriate
    /// for that host.
    ///
    /// Environment success returns immediately. Cancellation and timeout govern only subprocess
    /// discovery; a zero timeout rejects that path without spawning. No raw output is included in
    /// typed errors, and command failure does not return a token from unsuccessful stdout.
    ///
    /// # Errors
    ///
    /// Returns variable-name or token validation errors for invalid configured inputs. Helper
    /// spawn, exit, timeout, and cancellation have distinct variants; successful empty stdout
    /// returns [`CredentialError::NoCredential`]. Anonymous fallback remains the caller's
    /// policy.
    pub async fn resolve_token(
        &self,
        host: &GitHubHost,
        cancellation: &CancellationToken,
    ) -> Result<GitHubToken, CredentialError> {
        if let Some(name) = self.configured_token_environment_variable.as_deref()
            && !valid_environment_variable_name(name)
        {
            return Err(CredentialError::InvalidEnvironmentVariable);
        }

        let configured = self
            .configured_token_environment_variable
            .as_deref()
            .and_then(std::env::var_os);
        let github_token = std::env::var_os("GITHUB_TOKEN");
        if let Some(token) = choose_environment_token(configured, github_token)? {
            return Ok(token);
        }
        if self.command_timeout.is_zero() {
            return Err(CredentialError::TimedOut);
        }

        let arguments = [
            OsString::from("auth"),
            OsString::from("token"),
            OsString::from("--hostname"),
            OsString::from(host.as_str()),
        ];
        let output = run_credential_process(
            &self.gh_program,
            &arguments,
            self.command_timeout,
            cancellation,
        )
        .await?;
        let token =
            std::str::from_utf8(&output.stdout).map_err(|_| CredentialError::InvalidToken)?;
        let token = token.trim();
        if token.is_empty() {
            return Err(CredentialError::NoCredential);
        }
        GitHubToken::new(token).map_err(|_| CredentialError::InvalidToken)
    }
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
///
/// Shared with configuration validation but kept off the public credential-resolution API. It
/// checks spelling only; it does not read the environment, establish presence, or validate a token.
/// A separate module for this single predicate would add navigation without a broader concept.
pub(crate) fn valid_environment_variable_name(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

/// Executes a helper with captured stdout, no stdin, and discarded stderr.
///
/// `program` and `arguments` describe one direct executable invocation; no shell is inserted.
/// The timeout covers waiting for exit and captured output after spawn, not command construction
/// or environment lookup. Preexisting cancellation rejects the call before spawning. Cancellation
/// during waiting or timeout drops the child future with kill-on-drop enabled; this requests child
/// termination without waiting here for reaping or managing descendants.
///
/// Only a successful exit returns captured output. Spawn/wait/exit failures return typed categories
/// without including stdout or stderr; the caller separately validates token encoding and syntax.
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
mod tests;
