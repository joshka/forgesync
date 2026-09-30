//! A checked reference to a discussion within its repository.
//!
//! [`ThreadReference`] keeps the repository identity and positive thread number together. It is a
//! small domain locator for code that already has checked parts; it does not parse a CLI string or
//! contact GitHub.
//!
//! The CLI and engine accept human selectors such as `owner/repo#number` or a URL, then resolve
//! them against archive content. A repository rename changes its display path, not the stable
//! provider repository ID; a thread number is meaningful only within that repository. This locator
//! carries no discussion provider ID and proves no matching record exists. Keep human-selector
//! parsing in `forgesync-engine::reference` and use this type after its checked parts are known.
//!
//! Related identities live in [`crate::identity`]; content lives in [`crate::content`].

use serde::{Deserialize, Serialize};

use crate::identity::threads::{RepositoryId, ThreadNumber};

/// A parsed local thread reference after its repository scope is known.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ThreadReference {
    /// Stable host-qualified repository scope, independent of the current owner/name display path.
    repository: RepositoryId,
    /// Positive number within that scope; it is not an opaque discussion provider ID.
    number: ThreadNumber,
}

impl ThreadReference {
    /// Combines checked repository scope and number without resolving a discussion.
    ///
    /// Construction performs no archive lookup or provider request. It does not prove the numbered
    /// discussion exists; engine/store resolution owns that operation.
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
