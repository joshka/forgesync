//! Scoped identities for repositories, discussions, and child resources.
//!
//! Child IDs retain their supplied parent thread as part of equality and hashing. A repository path
//! may change while its provider identity stays stable, so owner/name strings are not a substitute
//! for [`RepositoryId`].

use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use crate::identity::IdentityError;
use crate::identity::host::GitHubHost;
use crate::identity::provider::ProviderId;

/// Positive issue or pull request number scoped to one repository; not a provider ID.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThreadNumber(NonZeroU64);

impl ThreadNumber {
    /// Checks positivity without selecting a repository or looking up a discussion.
    pub fn new(value: u64) -> Result<Self, IdentityError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(IdentityError::NotPositive)
    }

    pub fn get(self) -> u64 {
        self.0.get()
    }
}

/// Host-qualified stable repository identity.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct RepositoryId {
    host: GitHubHost,
    provider_id: ProviderId,
}

impl RepositoryId {
    pub fn new(host: GitHubHost, provider_id: ProviderId) -> Self {
        Self { host, provider_id }
    }

    pub fn host(&self) -> &GitHubHost {
        &self.host
    }

    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }
}

/// Stable issue or pull request identity scoped to its repository.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ThreadId {
    repository: RepositoryId,
    provider_id: ProviderId,
    number: ThreadNumber,
}

impl ThreadId {
    /// Combines checked parts; all three participate in equality and hashing.
    pub fn new(repository: RepositoryId, provider_id: ProviderId, number: ThreadNumber) -> Self {
        Self {
            repository,
            provider_id,
            number,
        }
    }

    pub fn repository(&self) -> &RepositoryId {
        &self.repository
    }

    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    pub fn number(&self) -> ThreadNumber {
        self.number
    }
}

/// Stable comment identity scoped to a discussion.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CommentId {
    thread: ThreadId,
    provider_id: ProviderId,
}

impl CommentId {
    pub fn new(thread: ThreadId, provider_id: ProviderId) -> Self {
        Self {
            thread,
            provider_id,
        }
    }

    pub fn thread(&self) -> &ThreadId {
        &self.thread
    }

    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }
}

/// Stable pull request review identity scoped to a discussion.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ReviewId {
    thread: ThreadId,
    provider_id: ProviderId,
}

impl ReviewId {
    pub fn new(thread: ThreadId, provider_id: ProviderId) -> Self {
        Self {
            thread,
            provider_id,
        }
    }

    pub fn thread(&self) -> &ThreadId {
        &self.thread
    }

    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }
}

/// Stable review-thread identity scoped to a pull request.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ReviewThreadId {
    thread: ThreadId,
    provider_id: ProviderId,
}

impl ReviewThreadId {
    pub fn new(thread: ThreadId, provider_id: ProviderId) -> Self {
        Self {
            thread,
            provider_id,
        }
    }

    pub fn thread(&self) -> &ThreadId {
        &self.thread
    }

    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }
}
