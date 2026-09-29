//! # Decide which observation becomes canonical
//!
//! The comparison functions order observations using source revision information and local
//! acquisition sequence. They are shared by parent-thread and child-family application, so
//! concurrent or delayed fetches use the same policy.
//!
//! Source time is not simply a last-write-wins wall clock: missing or equal source revisions need
//! a deterministic local tie break, while stale provider data should not replace newer canonical
//! evidence. Keep this rule centralized because a subtle change here affects every refresh and
//! retry path.

use std::cmp::Ordering;

use forgesync_core::identity::ObservationSequence;
use forgesync_core::observation::SourceClock;

use crate::error::StoreError;

/// Orders canonical observations by provider source clock, then acquisition sequence.
pub fn compare_observation_order(
    incoming_clock: &SourceClock,
    incoming_sequence: ObservationSequence,
    current_clock: &SourceClock,
    current_sequence: ObservationSequence,
) -> Result<Ordering, StoreError> {
    match compare_source_clocks(incoming_clock, current_clock)? {
        Ordering::Equal => Ok(incoming_sequence.cmp(&current_sequence)),
        order => Ok(order),
    }
}

/// Orders revision evidence by positive acquisition sequence, falling back to source clocks.
///
/// Older stored records can lack a sequence; `None` represents that legacy state. Distinct
/// malformed clocks remain ambiguous even when their acquisition sequences are different.
pub fn compare_revision_observation_order(
    incoming_clock: &SourceClock,
    incoming_sequence: Option<ObservationSequence>,
    current_clock: &SourceClock,
    current_sequence: Option<ObservationSequence>,
) -> Result<Ordering, StoreError> {
    if both_clocks_are_unusable(incoming_clock, current_clock)
        && normalized_unusable_clock(incoming_clock) != normalized_unusable_clock(current_clock)
    {
        return Err(ambiguous_clocks(incoming_clock, current_clock));
    }

    match (incoming_sequence, current_sequence) {
        (Some(incoming), Some(current)) if incoming != current => Ok(incoming.cmp(&current)),
        (Some(_), None) => Ok(Ordering::Greater),
        (None, Some(_)) => Ok(Ordering::Less),
        _ => compare_source_clocks(incoming_clock, current_clock),
    }
}

/// Converts a signed high-water value to the absolute order key used by legacy archives.
pub fn observation_sequence_order_value(sequence: i64) -> i64 {
    sequence.checked_abs().unwrap_or(i64::MAX)
}

/// Orders valid source clocks ahead of unusable ones. Distinct unusable spellings are ambiguous,
/// because choosing either one would make canonical content depend on arrival order.
fn compare_source_clocks(
    incoming: &SourceClock,
    current: &SourceClock,
) -> Result<Ordering, StoreError> {
    use SourceClock::{Invalid, Missing, Valid};

    match (incoming, current) {
        (Valid(incoming), Valid(current)) => Ok(incoming.cmp(current)),
        (Valid(_), Missing | Invalid(_)) => Ok(Ordering::Greater),
        (Missing | Invalid(_), Valid(_)) => Ok(Ordering::Less),
        (Missing, Missing) => Ok(Ordering::Equal),
        (Invalid(incoming), Invalid(current)) if incoming.trim() == current.trim() => {
            Ok(Ordering::Equal)
        }
        (Missing, Invalid(_)) | (Invalid(_), Missing) | (Invalid(_), Invalid(_)) => {
            Err(ambiguous_clocks(incoming, current))
        }
    }
}

/// Identifies clock pairs that need ambiguity protection before sequence ordering.
fn both_clocks_are_unusable(incoming: &SourceClock, current: &SourceClock) -> bool {
    !matches!(incoming, SourceClock::Valid(_)) && !matches!(current, SourceClock::Valid(_))
}

/// Retains a stable diagnostic form of an invalid or missing source clock.
fn normalized_unusable_clock(clock: &SourceClock) -> String {
    match clock {
        SourceClock::Missing => String::new(),
        SourceClock::Invalid(value) => value.trim().to_owned(),
        SourceClock::Valid(value) => value
            .format_rfc3339()
            .unwrap_or_else(|_| "invalid timestamp".to_owned()),
    }
}

/// Preserves both unusable clock values in the error so acquisition can report the conflict.
fn ambiguous_clocks(incoming: &SourceClock, current: &SourceClock) -> StoreError {
    StoreError::AmbiguousObservationClocks {
        incoming: normalized_unusable_clock(incoming),
        current: normalized_unusable_clock(current),
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use forgesync_core::identity::ObservationSequence;
    use forgesync_core::observation::SourceClock;

    use crate::ordering::{
        compare_observation_order, compare_revision_observation_order,
        observation_sequence_order_value,
    };

    fn sequence(value: u64) -> ObservationSequence {
        ObservationSequence::new(value).expect("positive sequence")
    }

    fn clock(value: Option<&str>) -> SourceClock {
        SourceClock::from_raw(value)
    }

    #[test]
    fn canonical_observation_order_uses_source_then_sequence() {
        let source_newer = compare_observation_order(
            &clock(Some("2026-09-20T10:00:01Z")),
            sequence(1),
            &clock(Some("2026-09-20T10:00:00Z")),
            sequence(2),
        )
        .expect("valid clocks are orderable");
        assert_eq!(source_newer, Ordering::Greater);

        let same_instant = compare_observation_order(
            &clock(Some("2026-09-20T11:00:00+01:00")),
            sequence(3),
            &clock(Some("2026-09-20T10:00:00Z")),
            sequence(2),
        )
        .expect("equivalent clocks are orderable");
        assert_eq!(same_instant, Ordering::Greater);

        let missing_clock =
            compare_observation_order(&clock(None), sequence(1), &clock(None), sequence(2))
                .expect("missing clocks use sequence");
        assert_eq!(missing_clock, Ordering::Less);
    }

    #[test]
    fn valid_clocks_beat_malformed_and_distinct_malformed_clocks_fail() {
        let valid_wins = compare_observation_order(
            &clock(Some("2026-09-20T10:00:00Z")),
            sequence(1),
            &clock(Some("not-a-time")),
            sequence(2),
        )
        .expect("valid clock outranks malformed clock");
        assert_eq!(valid_wins, Ordering::Greater);

        let error = compare_observation_order(
            &clock(Some("not-a-time-a")),
            sequence(2),
            &clock(Some("not-a-time-b")),
            sequence(1),
        )
        .expect_err("distinct invalid clocks are ambiguous");
        assert!(
            error
                .to_string()
                .contains("ambiguous malformed observation timestamps")
        );
    }

    #[test]
    fn revision_order_prefers_acquisition_sequence_and_supports_legacy_rows() {
        let fetch_wins = compare_revision_observation_order(
            &clock(Some("2026-09-20T10:00:00Z")),
            Some(sequence(2)),
            &clock(Some("2026-09-20T10:00:01Z")),
            Some(sequence(1)),
        )
        .expect("sequences determine revision order");
        assert_eq!(fetch_wins, Ordering::Greater);

        let source_fallback = compare_revision_observation_order(
            &clock(Some("2026-09-20T10:00:01Z")),
            None,
            &clock(Some("2026-09-20T10:00:00Z")),
            None,
        )
        .expect("legacy rows use source clocks");
        assert_eq!(source_fallback, Ordering::Greater);
    }

    #[test]
    fn signed_sequence_order_key_handles_minimum_integer() {
        assert_eq!(observation_sequence_order_value(i64::MIN), i64::MAX);
        assert_eq!(observation_sequence_order_value(-8), 8);
    }
}
