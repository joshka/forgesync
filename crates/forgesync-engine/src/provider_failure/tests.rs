//! # Provider classification scenarios
//!
//! Explicit typed inputs map to explicit domain categories. The cancellation case documents the
//! fallback conversion, not permission for a workflow to record cancellation as a failed request.
//! A separate diagnostic scenario verifies that the adapter preserves the typed display message.

use forgesync_core::coverage::FailureKind;
use forgesync_github::error::{ApiFailureKind, GitHubError};

use crate::provider_failure::github_failure;

#[rstest::rstest]
#[case::authentication(GitHubError::Api { status: 401, kind: ApiFailureKind::AuthenticationRequired }, FailureKind::Authentication)]
#[case::permission(GitHubError::Api { status: 403, kind: ApiFailureKind::PermissionDenied }, FailureKind::PermissionDenied)]
#[case::rate_limit(GitHubError::Api { status: 429, kind: ApiFailureKind::RateLimited }, FailureKind::RateLimited)]
#[case::deferred(GitHubError::Deferred { retry_after: None }, FailureKind::RateLimited)]
#[case::network(GitHubError::Network, FailureKind::Network)]
#[case::timeout(GitHubError::Timeout, FailureKind::Network)]
#[case::invalid_data(GitHubError::InvalidProviderData, FailureKind::InvalidData)]
#[case::cancelled(GitHubError::Cancelled, FailureKind::ProviderResponse)]
#[case::invalid_json(GitHubError::InvalidJson, FailureKind::ProviderResponse)]
#[case::server(GitHubError::Api { status: 500, kind: ApiFailureKind::Server }, FailureKind::ProviderResponse)]
fn typed_errors_keep_domain_categories(#[case] error: GitHubError, #[case] expected: FailureKind) {
    let failure = github_failure(&error);

    assert_eq!(failure.kind, expected);
}

#[test]
fn diagnostic_preserves_typed_provider_message() {
    let failure = github_failure(&GitHubError::GraphqlErrors { count: 2 });

    assert_eq!(
        failure.message,
        "GitHub GraphQL response contained 2 error(s)"
    );
}
