//! Local archive identities for runs and acquisition order.
//!
//! [`RunId`] names one durable workflow run. [`ObservationSequence`] orders locally acquired
//! evidence when source clocks alone cannot distinguish observations. Both wrap positive integers
//! and reject zero when constructed or deserialized.
//!
//! These IDs are assigned by store operations, not by GitHub. A sequence records local acquisition
//! order; it is not a source update time or a substitute for collection completeness. The store
//! uses it with [`crate::observation::SourceClock`] when deciding which observation is canonical.
//!
//! Use this module when passing a selected run to inspection or retry, or when retaining the
//! sequence reserved before acquisition. SQL range checks still belong to the store boundary.

use std::num::NonZeroU64;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::IdentityError;

/// Positive local archive run identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RunId(NonZeroU64);

impl RunId {
    /// Creates a checked positive run ID.
    pub fn new(value: u64) -> Result<Self, IdentityError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(IdentityError::InvalidRunId)
    }

    /// Returns the underlying ID.
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

impl Serialize for RunId {
    /// Encodes the positive archive-local run identity as an unsigned number.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.get())
    }
}

impl<'de> Deserialize<'de> for RunId {
    /// Rechecks positivity when decoding a run ID; SQL range validation remains a store concern.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// Positive acquisition sequence assigned before a family request starts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObservationSequence(NonZeroU64);

impl ObservationSequence {
    /// Creates a checked positive observation sequence.
    pub fn new(value: u64) -> Result<Self, IdentityError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(IdentityError::InvalidObservationSequence)
    }

    /// Returns the underlying sequence.
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

impl Serialize for ObservationSequence {
    /// Encodes local acquisition order as an unsigned number, separately from source timestamps.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.get())
    }
}

impl<'de> Deserialize<'de> for ObservationSequence {
    /// Rejects zero while decoding an acquisition sequence, preserving the constructor invariant.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}
