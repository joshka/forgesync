//! Incoming evidence with both source and acquisition context.
//!
//! Source time and local sequence are different ordering signals. An incomplete child collection
//! may record progress or failure, but cannot replace canonical complete membership or imply that
//! absent children were deleted. `forgesync-store::ordering` owns the replacement policy.

use serde::{Deserialize, Serialize};

use crate::content::Discussion;
use crate::identity::ObservationSequence;
use crate::timestamp::UtcTimestamp;

/// Provider source clock as retained in the archive, separate from local acquisition time.
///
/// Acquisition produces only `Valid`. `Missing` marks absent complete evidence, and `Invalid`
/// stays decodable from archive columns; ordering falls back to sequence for a missing clock and
/// rejects distinct malformed clocks as ambiguous.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceClock {
    /// The provider omitted its update timestamp.
    Missing,
    /// A valid provider source timestamp.
    Valid(UtcTimestamp),
    /// The provider supplied an invalid timestamp; its trimmed spelling is preserved for policy.
    Invalid(String),
}

/// Completeness claim for one independently acquired family scope.
///
/// An empty complete collection can establish that the scope has no members. An incomplete
/// collection, even with zero received items, cannot establish deletion of earlier membership.
/// The claim says what acquisition obtained, not whether archive ordering later accepts it.
#[derive(Clone, Debug, Eq, PartialEq)]
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
    /// The operation was cancelled during acquisition.
    Cancelled,
    /// The retry budget ended before the provider wait elapsed.
    RetryBudget,
    /// The response was incomplete for another provider-specific reason.
    Unknown,
}

/// One complete issue or pull-request snapshot acquired by a repository scan.
///
/// The store derives the source clock from `discussion.updated_at`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadObservation {
    pub discussion: Discussion,
    /// Local acquisition instant, used for coverage diagnostics rather than revision order.
    pub observed_at: UtcTimestamp,
    /// Archive-reserved acquisition order, allocated before provider I/O.
    pub sequence: ObservationSequence,
}
