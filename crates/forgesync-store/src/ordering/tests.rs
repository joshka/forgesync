//! Source-clock and acquisition-sequence ordering cases.

use std::cmp::Ordering;

use forgesync_core::identity::ObservationSequence;
use forgesync_core::observation::SourceClock;

use crate::error::StoreError;
use crate::ordering::compare_observation_order;

/// Constructs checked acquisition order without reserving archive state.
fn sequence(value: u64) -> ObservationSequence {
    ObservationSequence::new(value).expect("positive sequence")
}

/// Preserves valid, missing, and malformed source spellings as comparison inputs.
fn clock(value: Option<&str>) -> SourceClock {
    SourceClock::from_raw(value)
}

#[test]
fn canonical_order_prefers_newer_source_over_later_acquisition() {
    let source_newer = compare_observation_order(
        &clock(Some("2026-09-20T10:00:01Z")),
        sequence(1),
        &clock(Some("2026-09-20T10:00:00Z")),
        sequence(2),
    )
    .expect("valid clocks are orderable");
    assert_eq!(source_newer, Ordering::Greater);
}

#[test]
fn equivalent_source_instants_use_acquisition_order() {
    let same_instant = compare_observation_order(
        &clock(Some("2026-09-20T11:00:00+01:00")),
        sequence(3),
        &clock(Some("2026-09-20T10:00:00Z")),
        sequence(2),
    )
    .expect("equivalent clocks are orderable");
    assert_eq!(same_instant, Ordering::Greater);
}

#[test]
fn missing_source_clocks_use_acquisition_order() {
    let missing_clock =
        compare_observation_order(&clock(None), sequence(1), &clock(None), sequence(2))
            .expect("missing clocks use sequence");
    assert_eq!(missing_clock, Ordering::Less);
}

#[test]
fn valid_source_clock_outranks_malformed_source_clock() {
    let valid_wins = compare_observation_order(
        &clock(Some("2026-09-20T10:00:00Z")),
        sequence(1),
        &clock(Some("not-a-time")),
        sequence(2),
    )
    .expect("valid clock outranks malformed clock");
    assert_eq!(valid_wins, Ordering::Greater);
}

#[test]
fn distinct_malformed_clocks_preserve_both_values_in_typed_error() {
    let error = compare_observation_order(
        &clock(Some("not-a-time-a")),
        sequence(2),
        &clock(Some("not-a-time-b")),
        sequence(1),
    )
    .expect_err("distinct invalid clocks are ambiguous");
    let StoreError::AmbiguousObservationClocks { incoming, current } = error else {
        panic!("expected ambiguous observation clocks");
    };
    assert_eq!(incoming, "not-a-time-a");
    assert_eq!(current, "not-a-time-b");
}
