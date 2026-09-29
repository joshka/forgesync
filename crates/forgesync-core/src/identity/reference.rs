//! Reference identities.

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
