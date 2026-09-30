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

#[test]
fn credential_setup_failure_retains_invalid_token_cause() {
    let error = GitHubClientSetupError::Credential(CredentialError::InvalidToken);
    let source = error.source().expect("credential cause");

    assert_eq!(
        source.downcast_ref::<CredentialError>(),
        Some(&CredentialError::InvalidToken)
    );
}

#[test]
fn endpoint_setup_failure_retains_relative_url_cause() {
    let error = GitHubClientSetupError::InvalidApiUrl(url::ParseError::RelativeUrlWithoutBase);
    let source = error.source().expect("URL parsing cause");

    assert!(matches!(
        source.downcast_ref::<url::ParseError>(),
        Some(url::ParseError::RelativeUrlWithoutBase)
    ));
}

#[test]
fn adapter_setup_failure_retains_network_cause() {
    let error = GitHubClientSetupError::Initialization(GitHubError::Network);
    let source = error.source().expect("adapter cause");

    assert!(matches!(
        source.downcast_ref::<GitHubError>(),
        Some(GitHubError::Network)
    ));
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
