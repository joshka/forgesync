//! Checked identities for provider objects and local archive work.
//!
//! Provider parsing and CLI selectors validate before constructing these values. A store method may
//! then rely on the checked shape, but it still decides whether an identified record exists.

use thiserror::Error;

/// Errors returned when constructing a checked identity.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum IdentityError {
    #[error("GitHub host must be an HTTPS host name or IP address without credentials or a path")]
    InvalidGitHubHost,
    #[error("provider ID must be non-empty and contain no whitespace or control characters")]
    InvalidProviderId,
    #[error("commit SHA must contain 40 or 64 hexadecimal characters")]
    InvalidCommitSha,
    #[error("numeric ID must be greater than zero")]
    NotPositive,
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
