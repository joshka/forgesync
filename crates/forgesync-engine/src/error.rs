use forgesync_core::Failure;
use forgesync_github::{ApiFailureKind, GitHubError};
use forgesync_store::StoreError;
use thiserror::Error;

use crate::reference::ReferenceParseError;

/// Errors returned by local Forgesync workflows.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EngineError {
    /// A command supplied a malformed repository or thread selector.
    #[error(transparent)]
    Reference(#[from] ReferenceParseError),
    /// A selected repository is not registered in the archive.
    #[error("repository is not present in the archive: {host}/{owner}/{name}")]
    RepositoryMissing {
        /// Canonical GitHub host.
        host: String,
        /// Current owner name.
        owner: String,
        /// Current repository name.
        name: String,
    },
    /// A selected thread is not present in the archive.
    #[error("thread is not present in the archive")]
    ThreadMissing,
    /// A selected durable run does not exist in the archive.
    #[error("run {id} is not present in the archive")]
    RunMissing {
        /// Missing local run ID.
        id: u64,
    },
    /// A durable run has no unresolved failures to retry.
    #[error("run {id} has no unresolved retryable work")]
    NoRetryableWork {
        /// Run with no unresolved failures.
        id: u64,
    },
    /// A durable failure target cannot be mapped to a repository selector.
    #[error("run failure target cannot be retried: {target}")]
    RetryTargetInvalid {
        /// Persisted repository or selector value.
        target: String,
    },
    /// Sync requires either a non-empty explicit repository list or `--all`.
    #[error("sync requires repositories or --all, but not both")]
    InvalidSyncScope,
    /// No GitHub API client was provided for a selected host.
    #[error("no GitHub API client was configured for host {host}")]
    GitHubClientMissing {
        /// Selected GitHub host.
        host: String,
    },
    /// A search request has no searchable terms or an invalid advanced expression.
    #[error("search query is invalid")]
    InvalidSearchQuery,
    /// A page limit is zero or exceeds the local safety bound.
    #[error("page limit must be between 1 and 1000")]
    InvalidPageLimit,
    /// Pagination exceeds the range supported by SQLite.
    #[error("page offset is outside the supported range")]
    InvalidPageOffset,
    /// An archive operation failed.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A GitHub acquisition request failed before a partial report was available.
    #[error(transparent)]
    GitHub(#[from] GitHubError),
    /// A provider failure occurred but could not be written to the durable failure ledger.
    #[error("provider failure {original:?}; failure ledger write failed: {source}")]
    FailureLedger {
        /// Original provider failure that needed to be preserved for retry.
        original: Failure,
        /// Error returned while recording that failure.
        #[source]
        source: StoreError,
    },
}

impl EngineError {
    /// Returns the stable machine-readable classification for process output.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Reference(_) => "reference_invalid",
            Self::RepositoryMissing { .. } => "repository_missing",
            Self::ThreadMissing => "thread_missing",
            Self::RunMissing { .. } => "run_missing",
            Self::NoRetryableWork { .. } => "run_no_retryable_work",
            Self::RetryTargetInvalid { .. } => "run_retry_target_invalid",
            Self::InvalidSyncScope => "sync_scope_invalid",
            Self::GitHubClientMissing { .. } => "github_client_missing",
            Self::InvalidSearchQuery => "search_query_invalid",
            Self::InvalidPageLimit => "page_limit_invalid",
            Self::InvalidPageOffset => "page_offset_invalid",
            Self::Store(StoreError::InvalidSearchQuery) => "search_query_invalid",
            Self::Store(error) => error.code(),
            Self::GitHub(GitHubError::Cancelled) => "operation_cancelled",
            Self::GitHub(GitHubError::Timeout) => "github_timeout",
            Self::GitHub(GitHubError::Network) => "github_network_error",
            Self::GitHub(GitHubError::Api {
                kind: ApiFailureKind::AuthenticationRequired,
                ..
            }) => "github_authentication_required",
            Self::GitHub(GitHubError::Api {
                kind: ApiFailureKind::PermissionDenied,
                ..
            }) => "github_permission_denied",
            Self::GitHub(GitHubError::Deferred { .. }) => "github_retry_deferred",
            Self::GitHub(error) => match error {
                GitHubError::UntrustedOrigin => "github_untrusted_origin",
                GitHubError::InvalidPaginationLink => "github_pagination_invalid",
                GitHubError::RedirectRejected => "github_redirect_rejected",
                GitHubError::ResponseTooLarge => "github_response_too_large",
                GitHubError::InvalidJson => "github_response_invalid_json",
                GitHubError::GraphqlErrors { .. } => "github_graphql_errors",
                GitHubError::InvalidProviderData => "github_provider_data_invalid",
                GitHubError::ConcurrencyUnavailable => "github_concurrency_unavailable",
                GitHubError::InvalidApiBaseUrl => "github_api_url_invalid",
                GitHubError::InvalidConfiguration => "github_configuration_invalid",
                GitHubError::ClientInitialization => "github_client_initialization_failed",
                GitHubError::Api { .. } => "github_api_error",
                GitHubError::Cancelled
                | GitHubError::Timeout
                | GitHubError::Network
                | GitHubError::Deferred { .. } => unreachable!(),
            },
            Self::FailureLedger { .. } => "failure_ledger_write_failed",
        }
    }
}
