//! A checked reference to a discussion within its repository.
//!
//! [`ThreadReference`] keeps the repository identity and positive thread number together. It is a
//! small domain locator for code that already has checked parts; it does not parse a CLI string or
//! contact GitHub.
//!
//! The CLI and engine accept human selectors such as `owner/repo#number` or a URL, then resolve
//! them against archive content. The resulting provider discussion identity can differ from the
//! visible number if a repository is renamed. Keep parsing in `forgesync-engine::reference` and
//! use this type only after the constituent identities are known.
//!
//! Related identities live in [`crate::identity`]; content lives in [`crate::content`].

use serde::{Deserialize, Serialize};

use super::{RepositoryId, ThreadNumber};

/// A parsed local thread reference after its repository scope is known.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ThreadReference {
    repository: RepositoryId,
    number: ThreadNumber,
}

impl ThreadReference {
    /// Creates a repository-qualified thread reference.
    pub fn new(repository: RepositoryId, number: ThreadNumber) -> Self {
        Self { repository, number }
    }

    /// Returns the host-qualified repository.
    pub fn repository(&self) -> &RepositoryId {
        &self.repository
    }

    /// Returns the issue or pull request number.
    pub fn number(&self) -> ThreadNumber {
        self.number
    }
}
