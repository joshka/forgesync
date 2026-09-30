//! Incoming evidence with both source and acquisition context.
//!
//! [`Observation<T>`] pairs normalized content with its evidence family, source clock, local
//! sequence, acquisition time, and collection completeness. [`SourceClock`] distinguishes valid,
//! missing, and invalid provider time instead of collapsing them into one optional timestamp.
//! [`CollectionCompleteness`] distinguishes a finished page set from a partial one;
//! [`IncompleteReason`] retains why acquisition stopped.
//!
//! The engine constructs an observation after provider acquisition, and the store compares it with
//! current evidence before applying it. Source time and local sequence are different ordering
//! signals. An incomplete child collection may record progress or failure, but cannot replace
//! canonical complete membership or imply that absent children were deleted.
//!
//! Read [`crate::coverage`] for the resulting family state and `forgesync-store::ordering` for the
//! archive replacement policy. This module describes the claim carried by incoming evidence, not
//! the SQL transaction that accepts it.

use serde::{Deserialize, Serialize};

use crate::coverage::EvidenceFamily;
use crate::identity::ObservationSequence;
use crate::timestamp::UtcTimestamp;

/// Provider source-clock value retained separately from local acquisition time.
///
/// Missing and invalid values remain distinct because archive ordering policy may use a missing
/// clock with sequence fallback while rejecting an ambiguous malformed clock. This enum records
/// provider evidence; it does not select the canonical observation or guarantee replacement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "value", rename_all = "snake_case")]
pub enum SourceClock {
    /// The provider omitted its update timestamp.
    Missing,
    /// A valid provider source timestamp.
    Valid(UtcTimestamp),
    /// The provider supplied an invalid timestamp; its trimmed spelling is preserved for policy.
    Invalid(String),
}

impl SourceClock {
    /// Captures optional provider text without conflating missing and malformed clocks.
    ///
    /// Trims surrounding whitespace. Absent, empty, and whitespace-only values become
    /// [`Self::Missing`]. Valid RFC 3339 text becomes a UTC microsecond instant; malformed or
    /// unsupported timestamps become [`Self::Invalid`] with the trimmed spelling retained.
    /// This conversion never substitutes local acquisition time for a missing provider clock.
    ///
    /// ```
    /// use forgesync_core::observation::SourceClock;
    ///
    /// assert_eq!(SourceClock::from_raw(Some("  ")), SourceClock::Missing);
    /// assert_eq!(
    ///     SourceClock::from_raw(Some(" invalid ")),
    ///     SourceClock::Invalid("invalid".into())
    /// );
    /// ```
    pub fn from_raw(value: Option<&str>) -> Self {
        let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
            return Self::Missing;
        };

        match UtcTimestamp::parse(value) {
            Ok(timestamp) => Self::Valid(timestamp),
            Err(_) => Self::Invalid(value.to_owned()),
        }
    }
}

/// Completeness claim for one independently acquired family scope.
///
/// An empty complete collection can establish that the scope has no members. An incomplete
/// collection, even with zero received items, cannot establish deletion of earlier membership.
/// The claim says what acquisition obtained, not whether archive ordering later accepts it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CollectionCompleteness {
    /// Every page in the requested scope was acquired and validated.
    Complete,
    /// The result contains some data, but acquisition ended before completion.
    Incomplete {
        /// Why the collection could not be completed.
        reason: IncompleteReason,
        /// Number of items received before collection stopped.
        received_items: u64,
    },
}

/// Cause recorded when a collection is incomplete.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncompleteReason {
    /// A later REST or GraphQL page could not be acquired.
    Pagination,
    /// GraphQL returned usable data with one or more errors.
    ProviderPartialResponse,
    /// The operation was cancelled during acquisition.
    Cancelled,
    /// The retry budget ended before the provider wait elapsed.
    RetryBudget,
    /// The response was incomplete for another provider-specific reason.
    Unknown,
}

/// One normalized observation of one family, independent of archive storage.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Observation<T> {
    /// Independently acquired resource family to which the payload and completeness apply.
    family: EvidenceFamily,
    /// Normalized evidence; its concrete type determines whether it is one item or a collection.
    payload: T,
    /// Provider revision clock, retained independently of the local acquisition timestamp.
    source_clock: SourceClock,
    /// Local acquisition instant, used for diagnostics rather than replacing a provider revision.
    observed_at: UtcTimestamp,
    /// Archive-reserved acquisition ordering token, allocated before provider I/O.
    sequence: ObservationSequence,
    /// Explicit claim about completing the requested scope; empty payload alone proves nothing.
    completeness: CollectionCompleteness,
}

impl<T> Observation<T> {
    /// Packages normalized evidence with its explicit acquisition and source context.
    ///
    /// The constructor preserves all six independent domain facts. It performs no archive write,
    /// allocates no sequence, and does not inspect generic payload contents. In particular, callers
    /// must supply a truthful completeness claim and received-item count; this type cannot infer
    /// either from `T`. Reserve the sequence from the archive before acquisition.
    ///
    /// A complete empty collection can replace complete membership when store ordering permits it.
    /// An incomplete empty collection cannot assert that previously known members were deleted.
    pub fn new(
        family: EvidenceFamily,
        payload: T,
        source_clock: SourceClock,
        observed_at: UtcTimestamp,
        sequence: ObservationSequence,
        completeness: CollectionCompleteness,
    ) -> Self {
        Self {
            family,
            payload,
            source_clock,
            observed_at,
            sequence,
            completeness,
        }
    }

    /// Returns the family whose acquisition scope and completeness this observation describes.
    ///
    /// The generic payload is not inspected to validate its family. Normalization and archive
    /// boundaries must keep the concrete payload meaning aligned with this value.
    pub fn family(&self) -> EvidenceFamily {
        self.family
    }

    /// Borrows the normalized item or collection without consuming its acquisition context.
    ///
    /// Payload cardinality does not imply completeness; inspect [`Self::completeness`] before
    /// interpreting absent members as source deletions.
    pub fn payload(&self) -> &T {
        &self.payload
    }

    /// Borrows the provider revision clock, including missing or malformed source evidence.
    ///
    /// This is independent of [`Self::observed_at`]. Do not substitute acquisition time when the
    /// provider omitted its clock; the archive ordering policy owns fallback interpretation.
    pub fn source_clock(&self) -> &SourceClock {
        &self.source_clock
    }

    /// Returns the local instant recorded for acquisition diagnostics.
    ///
    /// This instant neither proves provider revision order nor grants writer ownership. The
    /// source clock and reserved sequence remain separate facts used by archive policy.
    pub fn observed_at(&self) -> UtcTimestamp {
        self.observed_at
    }

    /// Returns the archive-reserved ordering coordinate supplied by the caller.
    ///
    /// Construction does not prove that reservation happened. The coordinate is meaningful in
    /// its originating archive; it is not a globally comparable provider revision or timestamp.
    pub fn sequence(&self) -> ObservationSequence {
        self.sequence
    }

    /// Borrows the completeness claim, including why and how far an incomplete acquisition got.
    ///
    /// This describes the requested source scope, not whether a later store transaction accepted
    /// the observation or whether currently retained evidence is stale.
    pub fn completeness(&self) -> &CollectionCompleteness {
        &self.completeness
    }
}

#[cfg(test)]
mod tests;
