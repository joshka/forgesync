//! # Observation ordering cases
//!
//! These cases compare source revisions and local acquisition sequences under delayed or repeated
//! fetches. They protect canonical selection from simple last-write-wins behavior. A new ordering
//! rule should be backed by a fixture where the expected winner is explicit.

use std::cmp::Ordering;

use forgesync_core::identity::ObservationSequence;
use forgesync_core::observation::SourceClock;
use forgesync_store::ordering::{
    compare_observation_order, compare_revision_observation_order, observation_sequence_order_value,
};

#[test]
fn newer_source_time_wins_despite_an_older_acquisition_sequence() {
    let incoming = SourceClock::from_raw(Some("2026-09-20T10:00:01Z"));
    let current = SourceClock::from_raw(Some("2026-09-20T10:00:00Z"));
    let sequence_one = ObservationSequence::new(1).expect("sequence");
    let sequence_two = ObservationSequence::new(2).expect("sequence");
    assert_eq!(
        compare_observation_order(&incoming, sequence_one, &current, sequence_two)
            .expect("source order"),
        Ordering::Greater
    );
}

#[test]
fn minimum_legacy_sequence_maps_to_the_maximum_order_value() {
    assert_eq!(observation_sequence_order_value(i64::MIN), i64::MAX);
}

#[test]
fn legacy_revisions_without_sequences_compare_source_time() {
    let incoming = SourceClock::from_raw(Some("2026-09-20T10:00:01Z"));
    let current = SourceClock::from_raw(Some("2026-09-20T10:00:00Z"));

    let order = compare_revision_observation_order(&incoming, None, &current, None)
        .expect("legacy source order");

    assert_eq!(order, Ordering::Greater);
}

#[test]
fn revision_sequence_precedes_source_time_when_both_sequences_are_known() {
    let incoming = SourceClock::from_raw(Some("2026-09-20T10:00:00Z"));
    let current = SourceClock::from_raw(Some("2026-09-20T10:00:01Z"));
    let earlier = ObservationSequence::new(1).expect("earlier sequence");
    let later = ObservationSequence::new(2).expect("later sequence");

    let order = compare_revision_observation_order(&incoming, Some(later), &current, Some(earlier))
        .expect("revision sequence order");

    assert_eq!(order, Ordering::Greater);
}
