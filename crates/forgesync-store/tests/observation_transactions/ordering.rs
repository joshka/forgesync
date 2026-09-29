//! # Observation ordering cases
//!
//! These cases compare source revisions and local acquisition sequences under delayed or repeated
//! fetches. They protect canonical selection from simple last-write-wins behavior. A new ordering
//! rule should be backed by a fixture where the expected winner is explicit.

use super::{
    ObservationSequence, Ordering, SourceClock, compare_observation_order,
    compare_revision_observation_order, observation_sequence_order_value,
};

#[test]
fn observation_ordering_covers_source_precedence_legacy_fallback_and_min_sequence() {
    let incoming = SourceClock::from_raw(Some("2026-09-20T10:00:01Z"));
    let current = SourceClock::from_raw(Some("2026-09-20T10:00:00Z"));
    let sequence_one = ObservationSequence::new(1).expect("sequence");
    let sequence_two = ObservationSequence::new(2).expect("sequence");
    assert_eq!(
        compare_observation_order(&incoming, sequence_one, &current, sequence_two)
            .expect("source order"),
        Ordering::Greater
    );
    assert_eq!(observation_sequence_order_value(i64::MIN), i64::MAX);
    assert_eq!(
        compare_revision_observation_order(&incoming, None, &current, None)
            .expect("legacy source order"),
        Ordering::Greater
    );
}
