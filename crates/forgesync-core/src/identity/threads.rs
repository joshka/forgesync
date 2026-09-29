//! Scoped identities for repositories, discussions, and child resources.
//!
//! [`RepositoryId`] combines host and provider repository ID. [`ThreadId`] adds the
//! provider-issued discussion ID and a positive repository-local number. [`CommentId`],
//! [`ReviewId`], and [`ReviewThreadId`] retain their owning thread so a child cannot silently move
//! to another parent.
//!
//! These values join normalized content to observations and archive rows. Provider normalization
//! constructs them from checked response fields; the store converts them to SQL keys at its
//! boundary. A repository path may change while its provider identity stays stable, so owner/name
//! strings are not a substitute for [`RepositoryId`].
//!
//! Use the child-specific identity that matches the resource family. [`ThreadNumber`] is for
//! display and selectors; [`ProviderId`] is the source identity behind a row.

use std::num::NonZeroU64;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{GitHubHost, IdentityError, ProviderId};

/// Positive issue or pull request number scoped to one repository.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ThreadNumber(NonZeroU64);

impl ThreadNumber {
    /// Creates a checked positive thread number.
    pub fn new(value: u64) -> Result<Self, IdentityError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(IdentityError::InvalidThreadNumber)
    }

    /// Returns the underlying number.
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

impl Serialize for ThreadNumber {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.get())
    }
}

impl<'de> Deserialize<'de> for ThreadNumber {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Host-qualified stable repository identity.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct RepositoryId {
    host: GitHubHost,
    provider_id: ProviderId,
}

impl RepositoryId {
    /// Creates a repository identity from checked host and provider IDs.
    pub fn new(host: GitHubHost, provider_id: ProviderId) -> Self {
        Self { host, provider_id }
    }

    /// Returns the provider host.
    pub fn host(&self) -> &GitHubHost {
        &self.host
    }

    /// Returns the stable provider ID.
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
    /// Creates a thread identity with its host-qualified parent and display number.
    pub fn new(repository: RepositoryId, provider_id: ProviderId, number: ThreadNumber) -> Self {
        Self {
            repository,
            provider_id,
            number,
        }
    }

    /// Returns the containing repository identity.
    pub fn repository(&self) -> &RepositoryId {
        &self.repository
    }

    /// Returns the stable provider ID.
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    /// Returns the issue or pull request number.
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
    /// Creates a comment identity with its parent thread and provider ID.
    pub fn new(thread: ThreadId, provider_id: ProviderId) -> Self {
        Self {
            thread,
            provider_id,
        }
    }

    /// Returns the parent discussion identity.
    pub fn thread(&self) -> &ThreadId {
        &self.thread
    }

    /// Returns the stable provider ID.
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
    /// Creates a review identity with its parent pull request and provider ID.
    pub fn new(thread: ThreadId, provider_id: ProviderId) -> Self {
        Self {
            thread,
            provider_id,
        }
    }

    /// Returns the parent pull request identity.
    pub fn thread(&self) -> &ThreadId {
        &self.thread
    }

    /// Returns the stable provider ID.
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
    /// Creates a review-thread identity with its parent pull request and provider ID.
    pub fn new(thread: ThreadId, provider_id: ProviderId) -> Self {
        Self {
            thread,
            provider_id,
        }
    }

    /// Returns the parent pull request identity.
    pub fn thread(&self) -> &ThreadId {
        &self.thread
    }

    /// Returns the stable provider ID.
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }
}
