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
use crate::timestamp::{TimestampError, UtcTimestamp};

/// Provider source-clock value retained separately from local acquisition time.
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
    /// Captures an optional raw provider value while keeping invalid clocks explicit.
    pub fn from_raw(value: Option<&str>) -> Self {
        let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
            return Self::Missing;
        };

        match UtcTimestamp::parse(value) {
            Ok(timestamp) => Self::Valid(timestamp),
            Err(TimestampError::InvalidRfc3339) => Self::Invalid(value.to_owned()),
            Err(_) => Self::Invalid(value.to_owned()),
        }
    }
}

/// Completeness of one family collection result.
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
    family: EvidenceFamily,
    payload: T,
    source_clock: SourceClock,
    observed_at: UtcTimestamp,
    sequence: ObservationSequence,
    completeness: CollectionCompleteness,
}

impl<T> Observation<T> {
    /// Creates an observation while retaining its source and acquisition facts separately.
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

    /// Returns the independently covered family.
    pub fn family(&self) -> EvidenceFamily {
        self.family
    }

    /// Returns the normalized payload.
    pub fn payload(&self) -> &T {
        &self.payload
    }

    /// Returns the provider's source-clock state.
    pub fn source_clock(&self) -> &SourceClock {
        &self.source_clock
    }

    /// Returns local acquisition time.
    pub fn observed_at(&self) -> UtcTimestamp {
        self.observed_at
    }

    /// Returns the sequence reserved before acquisition.
    pub fn sequence(&self) -> ObservationSequence {
        self.sequence
    }

    /// Returns whether the collection completed in the requested scope.
    pub fn completeness(&self) -> &CollectionCompleteness {
        &self.completeness
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{CollectionCompleteness, Observation, SourceClock};
    use crate::coverage::EvidenceFamily;
    use crate::identity::ObservationSequence;
    use crate::observation::IncompleteReason;
    use crate::timestamp::UtcTimestamp;

    #[test]
    fn source_clock_keeps_missing_valid_and_invalid_states_distinct() {
        assert_eq!(SourceClock::from_raw(None), SourceClock::Missing);
        assert_eq!(SourceClock::from_raw(Some("  ")), SourceClock::Missing);
        assert_eq!(
            SourceClock::from_raw(Some("2026-09-20T11:00:00+01:00")),
            SourceClock::Valid(
                UtcTimestamp::parse("2026-09-20T10:00:00Z").expect("valid timestamp")
            )
        );
        assert_eq!(
            SourceClock::from_raw(Some(" not-a-time ")),
            SourceClock::Invalid("not-a-time".to_owned())
        );
    }

    #[test]
    fn observation_round_trip_keeps_family_time_sequence_and_completeness() {
        let observation = Observation::new(
            EvidenceFamily::Comments,
            vec!["comment-a".to_owned(), "comment-b".to_owned()],
            SourceClock::Missing,
            UtcTimestamp::parse("2026-09-20T10:00:00Z").expect("valid timestamp"),
            ObservationSequence::new(7).expect("positive sequence"),
            CollectionCompleteness::Incomplete {
                reason: IncompleteReason::Pagination,
                received_items: 2,
            },
        );

        let value = serde_json::to_value(&observation).expect("serialize observation");
        assert_eq!(value["family"], json!("comments"));
        assert_eq!(value["sequence"], json!(7));
        assert_eq!(value["completeness"]["status"], json!("incomplete"));
        assert_eq!(
            observation.observed_at(),
            UtcTimestamp::parse("2026-09-20T10:00:00Z").expect("valid timestamp")
        );

        let decoded: Observation<Vec<String>> =
            serde_json::from_value(value).expect("deserialize observation");
        assert_eq!(decoded, observation);
    }

    #[test]
    fn complete_empty_is_not_missing_or_incomplete() {
        let current = UtcTimestamp::parse("2026-09-20T10:00:00Z").expect("timestamp");
        let observed_at = UtcTimestamp::parse("2026-09-20T10:01:00Z").expect("timestamp");
        let complete_empty = Observation::new(
            EvidenceFamily::Comments,
            Vec::<String>::new(),
            SourceClock::Valid(current),
            observed_at,
            ObservationSequence::new(8).expect("sequence"),
            CollectionCompleteness::Complete,
        );

        assert!(matches!(
            complete_empty.completeness(),
            CollectionCompleteness::Complete
        ));
        assert_eq!(complete_empty.payload().len(), 0);
    }
}
