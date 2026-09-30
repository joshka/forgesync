//! # Classify GitHub failures for durable evidence
//!
//! [`github_failure`] converts provider-owned errors into the core failure vocabulary recorded
//! by acquisition workflows. Enumeration and child-family sync use the same mapping so identical
//! transport failures do not receive different ledger classifications depending on their caller.
//!
//! Authentication, permission, rate limiting, network, and invalid-data cases have specific kinds.
//! Remaining protocol/setup errors use provider-response classification. Cancellation also maps
//! there when converted, but workflow owners must interpret cancellation before choosing whether
//! to record a failure. This helper is not a retry or cancellation policy.
//!
//! The diagnostic message comes from the typed provider display implementation; this adapter
//! performs no additional redaction and never stores a raw response. Its private module boundary
//! keeps classification policy internal while allowing ordinary public visibility for callers.

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
        | GitHubError::ConcurrencyUnavailable
        | GitHubError::InvalidApiBaseUrl
        | GitHubError::InvalidConfiguration
        | GitHubError::ClientInitialization => FailureKind::ProviderResponse,
    };
    Failure {
        kind,
        message: error.to_string(),
    }
}
