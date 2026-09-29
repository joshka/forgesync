//! Checked identities for provider objects and local archive work.
//!
//! Choose a type for the meaning of the ID rather than passing unvalidated strings or bare
//! numbers. [`GitHubHost`] identifies an API authority; [`RepositoryId`] and [`ThreadId`] combine
//! provider identity with their parent scope. Comment, review, and review-thread IDs keep child
//! resources separate. [`ThreadNumber`] is a positive repository-local display number, not a
//! provider ID.
//!
//! [`ProviderId`] is opaque provider identity, while [`CommitSha`] validates a complete
//! hexadecimal revision. [`RunId`] and [`ObservationSequence`] belong to local archive work and
//! should not be confused with source timestamps. [`ThreadReference`] is a compact checked locator
//! for a thread.
//!
//! Provider parsing and CLI selectors should validate before constructing these values. A store
//! method may then rely on the checked shape, but it still decides whether an identified record
//! exists in its archive. The child files group host, provider, thread, and archive identities so
//! a reader can find the relevant constructor without a flat crate root.

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
