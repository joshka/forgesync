//! Checked provider and archive identities.

use thiserror::Error;

/// Errors returned when constructing a checked identity.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum IdentityError {
    /// A GitHub host was not a valid HTTPS host name or IP authority.
    #[error("GitHub host must be an HTTPS host name or IP address without credentials or a path")]
    InvalidGitHubHost,
    /// A provider-issued opaque ID was empty or contained whitespace/control characters.
    #[error("provider ID must be non-empty and contain no whitespace or control characters")]
    InvalidProviderId,
    /// A commit SHA did not use a supported full hexadecimal form.
    #[error("commit SHA must contain 40 or 64 hexadecimal characters")]
    InvalidCommitSha,
    /// A GitHub issue or pull request number must be positive.
    #[error("thread number must be greater than zero")]
    InvalidThreadNumber,
    /// A local archive run ID must be positive.
    #[error("run ID must be greater than zero")]
    InvalidRunId,
    /// An acquisition sequence must be positive.
    #[error("observation sequence must be greater than zero")]
    InvalidObservationSequence,
}

mod archive;
mod host;
mod provider;
mod reference;
mod threads;

pub use archive::{ObservationSequence, RunId};
pub use host::GitHubHost;
pub use provider::{CommitSha, ProviderId};
pub use reference::ThreadReference;
pub use threads::{CommentId, RepositoryId, ReviewId, ReviewThreadId, ThreadId, ThreadNumber};

#[cfg(test)]
mod tests;
