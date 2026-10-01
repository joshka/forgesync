//! A checked reference to a discussion within its repository.
//!
//! Human selectors such as `owner/repo#number` are parsed in `forgesync-engine::reference`; this
//! type holds the checked parts and carries no discussion provider ID.

use serde::{Deserialize, Serialize};

use crate::identity::threads::{RepositoryId, ThreadNumber};

/// A parsed local thread reference after its repository scope is known.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ThreadReference {
    repository: RepositoryId,
    number: ThreadNumber,
}

impl ThreadReference {
    pub fn new(repository: RepositoryId, number: ThreadNumber) -> Self {
        Self { repository, number }
    }

    pub fn repository(&self) -> &RepositoryId {
        &self.repository
    }

    pub fn number(&self) -> ThreadNumber {
        self.number
    }
}
