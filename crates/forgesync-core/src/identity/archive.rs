//! Local archive identities for runs and acquisition order.
//!
//! These IDs are assigned by store operations, not by GitHub. A sequence records local acquisition
//! order; it is not a source update time or a substitute for collection completeness.

use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use super::IdentityError;

/// Positive local archive run identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(NonZeroU64);

impl RunId {
    /// Checks positivity; the store separately checks SQLite range and record existence.
    ///
    /// ```
    /// use forgesync_core::identity::{IdentityError, RunId};
    ///
    /// assert_eq!(RunId::new(17)?.get(), 17);
    /// assert_eq!(RunId::new(0), Err(IdentityError::NotPositive));
    /// # Ok::<(), IdentityError>(())
    /// ```
    pub fn new(value: u64) -> Result<Self, IdentityError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(IdentityError::NotPositive)
    }

    pub fn get(self) -> u64 {
        self.0.get()
    }
}

/// Positive acquisition sequence reserved from the archive before a request starts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObservationSequence(NonZeroU64);

impl ObservationSequence {
    /// Checks positivity without reserving the sequence in any archive.
    pub fn new(value: u64) -> Result<Self, IdentityError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(IdentityError::NotPositive)
    }

    pub fn get(self) -> u64 {
        self.0.get()
    }
}
