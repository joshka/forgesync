//! # Independent source and acquisition ordering contracts
//!
//! Each named case compares one incoming/current pair and states its ordering directly.
//! Canonical content prioritizes source revision; revision evidence can prioritize acquisition.
//! Equivalent time zones and missing clocks exercise sequence tie-breaking independently.
//! A typed-error case retains both malformed spellings rather than depending on display text.
//!
//! Fixture helpers only construct checked sequence values and source clocks; they perform no I/O.
//! Comparisons do not prove that a sequence was reserved or that content was committed.
//! Archive integration tests establish those persistence and completeness boundaries separately.
//! Signed legacy order-key cases cover overflow and ordinary negative values independently.

use std::cmp::Ordering;

use forgesync_core::identity::ObservationSequence;
use forgesync_core::observation::SourceClock;

use crate::error::StoreError;
use crate::ordering::{
    compare_observation_order, compare_revision_observation_order, observation_sequence_order_value,
};

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

#[test]
fn revision_order_prefers_later_acquisition_over_newer_source() {
    let fetch_wins = compare_revision_observation_order(
        &clock(Some("2026-09-20T10:00:00Z")),
        Some(sequence(2)),
        &clock(Some("2026-09-20T10:00:01Z")),
        Some(sequence(1)),
    )
    .expect("sequences determine revision order");
    assert_eq!(fetch_wins, Ordering::Greater);
}

#[test]
fn revision_order_without_sequences_falls_back_to_source_clock() {
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
}

#[test]
fn signed_negative_sequence_uses_absolute_order_key() {
    assert_eq!(observation_sequence_order_value(-8), 8);
}
