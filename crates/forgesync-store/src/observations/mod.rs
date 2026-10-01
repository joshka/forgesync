//! Ordered source observations: sequence allocation, repository registration, and canonical
//! thread application, plus the result types shared with child-family acquisition.

use forgesync_core::identity::{ObservationSequence, ProviderId};
use serde::{Deserialize, Serialize};

/// The disposition of an observation or family reservation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationDisposition {
    /// The observation changed canonical content, evidence, membership, or coverage.
    Applied,
    /// The same observation had already been applied.
    Replayed,
    /// A newer reservation or accepted generation made this observation stale.
    Skipped,
}

/// Result of applying one canonical thread observation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ThreadObservationResult {
    /// Stable row ID for use by later local queries.
    pub thread_row_id: i64,
    /// Whether canonical content or complete evidence changed.
    pub disposition: ObservationDisposition,
    /// Highest sequence observed for the current source generation.
    pub high_water_sequence: ObservationSequence,
    /// Sequence of the last accepted complete parent observation, if any.
    pub evidence_sequence: Option<ObservationSequence>,
}

/// One stable provider item staged for a child-family collection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StagedItem<T> {
    /// Provider-issued identity within this family and parent thread.
    pub id: ProviderId,
    /// Normalized item value serialized at the store boundary.
    pub payload: T,
}

/// Result of reserving a sequence for one independently refreshed family.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FamilyReservation {
    /// Sequence allocated before acquisition.
    pub sequence: ObservationSequence,
    /// False when a newer source generation or acquisition already owns the family reservation.
    pub reserved: bool,
}

/// Result of completing or recording an incomplete child-family collection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FamilyObservationResult {
    /// Whether this generation changed current membership or coverage.
    pub disposition: ObservationDisposition,
    /// Number of distinct provider items staged for this generation.
    pub item_count: u64,
}

mod apply;
mod coverage;
mod repository;
mod sequence;
