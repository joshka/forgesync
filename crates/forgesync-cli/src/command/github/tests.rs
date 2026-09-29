//! # Provider setup failure source contracts
//!
//! Sync, refresh, and retry share these failures before rendering their command-specific envelope.
//! Static credential, endpoint, and adapter cases inspect `Error::source` at that earlier boundary,
//! proving that diagnostic causes survive until rendering rather than becoming opaque strings.
//!
//! Cases need no environment variables, credential helper, tokens, or network requests.
//! Cancellation has no underlying cause because it is an explicit caller signal. Its display text
//! stays safe for process diagnostics; CLI integration scenarios separately check rendered status
//! and command output.

use std::error::Error;

use forgesync_github::error::GitHubError;

use crate::command::github::GitHubClientSetupError;
use crate::credentials::CredentialError;

#[rstest::rstest]
#[case::credential(
    GitHubClientSetupError::Credential(CredentialError::InvalidToken),
    CredentialError::InvalidToken.to_string()
)]
#[case::endpoint(
    GitHubClientSetupError::InvalidApiUrl(url::ParseError::RelativeUrlWithoutBase),
    url::ParseError::RelativeUrlWithoutBase.to_string()
)]
#[case::adapter(
    GitHubClientSetupError::Initialization(GitHubError::Network),
    GitHubError::Network.to_string()
)]
fn setup_failure_retains_its_typed_cause(
    #[case] error: GitHubClientSetupError,
    #[case] message: String,
) {
    let source = error.source().expect("typed setup cause");

    assert_eq!(source.to_string(), message);
}

#[test]
fn cancellation_is_a_distinct_failure_without_an_underlying_cause() {
    let error = GitHubClientSetupError::Cancelled;

    assert!(error.source().is_none());
    assert_eq!(
        error.to_string(),
        "operation was cancelled before GitHub acquisition began"
    );
}
