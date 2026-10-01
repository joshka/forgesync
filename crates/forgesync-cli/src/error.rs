//! Process failures and exit statuses.
//!
//! Commands return [`CliError`] and the dispatcher renders it once, so every failure keeps one
//! stable code, safe message, and exit status regardless of which command produced it.

use std::process::ExitCode;

use forgesync_engine::embedding_client::EmbeddingClientError;
use forgesync_engine::error::EngineError;
use forgesync_engine::refresh::{RefreshStageFailure, RefreshStageStatus};
use forgesync_github::error::GitHubError;
use forgesync_store::error::StoreError;

use crate::config::ConfigError;
use crate::credentials::CredentialError;

/// Process exit statuses shared by every command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Exit {
    Success,
    Failure,
    /// Invalid arguments or configuration (2).
    Usage,
    /// Partial or deferred work that can be retried (3).
    Partial,
    /// Cancelled by the user, following the shell's SIGINT convention (130).
    Interrupted,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> Self {
        match exit {
            Exit::Success => Self::SUCCESS,
            Exit::Failure => Self::FAILURE,
            Exit::Usage => Self::from(2),
            Exit::Partial => Self::from(3),
            Exit::Interrupted => Self::from(130),
        }
    }
}

/// Any failure a command can report.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// Argument combinations Clap cannot express; rendered as a Clap usage diagnostic.
    #[error("{0}")]
    Usage(&'static str),
    /// Rejected arguments reported through the normal output envelope.
    #[error("{message}")]
    InvalidArguments {
        code: &'static str,
        message: &'static str,
    },
    #[error("{0}")]
    Config(#[from] ConfigError),
    #[error("{0}")]
    Store(#[from] StoreError),
    #[error("{0}")]
    Engine(#[from] EngineError),
    /// Credential discovery was interrupted before acquisition began.
    #[error("operation was cancelled before GitHub acquisition began")]
    SetupCancelled,
    #[error("{0}")]
    Credential(#[source] CredentialError),
    #[error("could not build GitHub API URL")]
    GitHubApiUrl(#[source] url::ParseError),
    #[error("{0}")]
    GitHubClient(#[source] GitHubError),
    #[error("{0}")]
    EmbeddingClient(#[from] EmbeddingClientError),
    /// An embedding stage finished without a report.
    #[error("{}", .failure.message)]
    Stage {
        status: RefreshStageStatus,
        failure: RefreshStageFailure,
    },
    #[cfg(feature = "tui")]
    #[error("{0}")]
    Tui(#[from] forgesync_tui::TuiError),
    #[error("could not start async runtime: {0}")]
    Runtime(#[source] std::io::Error),
}

impl CliError {
    /// Stable machine-readable classification for the JSON error envelope.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Usage(_) => "usage_invalid",
            Self::InvalidArguments { code, .. } => code,
            Self::Config(error) => error.code(),
            Self::Store(error) => error.code(),
            Self::Engine(error) => error.code(),
            Self::SetupCancelled => "operation_cancelled",
            Self::Credential(_) => "github_credential_invalid",
            Self::GitHubApiUrl(_) => "github_api_url_invalid",
            Self::GitHubClient(_) => "github_client_initialization_failed",
            Self::EmbeddingClient(error) => error.code(),
            Self::Stage { failure, .. } => failure.code,
            #[cfg(feature = "tui")]
            Self::Tui(error) => error.code(),
            Self::Runtime(_) => "runtime_unavailable",
        }
    }

    /// Exit status for this failure; cancellation keeps the shell's 130 convention.
    pub fn exit(&self) -> Exit {
        match self {
            Self::Usage(_) | Self::InvalidArguments { .. } | Self::Config(_) => Exit::Usage,
            Self::SetupCancelled
            | Self::Stage {
                status: RefreshStageStatus::Interrupted,
                ..
            } => Exit::Interrupted,
            Self::Engine(error) if error.is_cancelled() => Exit::Interrupted,
            _ => Exit::Failure,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use forgesync_github::error::GitHubError;

    use super::{CliError, Exit};
    use crate::credentials::CredentialError;

    #[test]
    fn setup_cancellation_is_an_interrupted_error() {
        let error = CliError::SetupCancelled;

        assert_eq!(error.code(), "operation_cancelled");
        assert_eq!(error.exit(), Exit::Interrupted);
        assert!(error.source().is_none());
    }

    #[test]
    fn setup_failures_retain_their_typed_causes() {
        let credential = CliError::Credential(CredentialError::InvalidToken);
        let url = CliError::GitHubApiUrl(url::ParseError::RelativeUrlWithoutBase);
        let client = CliError::GitHubClient(GitHubError::Network);

        assert_eq!(
            credential
                .source()
                .and_then(|source| source.downcast_ref::<CredentialError>()),
            Some(&CredentialError::InvalidToken)
        );
        assert!(
            url.source()
                .is_some_and(|source| source.is::<url::ParseError>())
        );
        assert!(matches!(
            client
                .source()
                .and_then(|source| source.downcast_ref::<GitHubError>()),
            Some(GitHubError::Network)
        ));
    }
}
