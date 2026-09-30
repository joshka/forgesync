//! # Ordered source observations and coverage
//!
//! A thread observation is evidence acquired at a particular local sequence with a source clock.
//! `ObservationDisposition` tells callers whether it became canonical; `ThreadObservationResult`
//! reports the application. `FamilyReservation`, `FamilyObservationResult`, and `StagedItem`
//! support child-family acquisition without conflating it with the parent snapshot.
//!
//! `sequence` allocates durable local order, `apply` commits a parent snapshot, `repository`
//! resolves its scope, and `coverage` reads directly recorded family completeness. Source update
//! time and acquisition order have different jobs: an older provider revision should not displace
//! newer canonical content merely because it arrived later. The engine decides when to fetch; the
//! store enforces these ordering and completeness rules.

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
mod thread_rows;
