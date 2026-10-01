//! Classify GitHub failures for durable evidence.
//!
//! Enumeration and child-family sync share this mapping so identical transport failures get the
//! same ledger classification. Callers must handle cancellation before deciding to record one.

use forgesync_core::coverage::{Failure, FailureKind};
use forgesync_github::error::{ApiFailureKind, GitHubError};

/// Converts a typed provider error to structured family failure evidence.
pub fn github_failure(error: &GitHubError) -> Failure {
    let kind = match error {
        GitHubError::Api {
            kind: ApiFailureKind::AuthenticationRequired,
            ..
        } => FailureKind::Authentication,
        GitHubError::Api {
            kind: ApiFailureKind::PermissionDenied,
            ..
        } => FailureKind::PermissionDenied,
        GitHubError::Deferred { .. }
        | GitHubError::Api {
            kind: ApiFailureKind::RateLimited,
            ..
        } => FailureKind::RateLimited,
        GitHubError::Network | GitHubError::Timeout => FailureKind::Network,
        GitHubError::InvalidProviderData => FailureKind::InvalidData,
        GitHubError::Cancelled
        | GitHubError::Api { .. }
        | GitHubError::UntrustedOrigin
        | GitHubError::InvalidPaginationLink
        | GitHubError::RedirectRejected
        | GitHubError::ResponseTooLarge
        | GitHubError::InvalidJson
        | GitHubError::GraphqlErrors { .. }
        | GitHubError::InvalidApiBaseUrl
        | GitHubError::InvalidConfiguration
        | GitHubError::ClientInitialization => FailureKind::ProviderResponse,
    };
    Failure {
        kind,
        message: error.to_string(),
    }
}

#[cfg(test)]
#[path = "provider_failure/tests.rs"]
mod tests;
