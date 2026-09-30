//! # Source clock and observation representation contracts
//!
//! These scenarios distinguish missing provider time, normalized valid time, and retained malformed
//! spelling. Observation serialization keeps family, sequence, and completeness independent.
//! Complete-empty evidence has its own direct expectation because cardinality alone cannot explain
//! whether acquisition completed.
//!
//! Tests construct values without reserving real archive sequences or contacting a provider. Store
//! integration suites own ordering acceptance and durable membership; these cases own the core
//! value boundary. Named input cases keep each normalization expectation visible without scenario
//! loops.

use rstest::rstest;
use serde_json::json;

use crate::coverage::EvidenceFamily;
use crate::identity::ObservationSequence;
use crate::observation::{CollectionCompleteness, IncompleteReason, Observation, SourceClock};
use crate::timestamp::UtcTimestamp;

#[rstest]
#[case::absent(None)]
#[case::empty(Some(""))]
#[case::whitespace(Some("  "))]
fn absent_source_text_has_no_provider_clock(#[case] raw: Option<&str>) {
    assert_eq!(SourceClock::from_raw(raw), SourceClock::Missing);
}

#[test]
fn offset_source_time_normalizes_to_the_same_utc_instant() {
    let expected = UtcTimestamp::parse("2026-09-20T10:00:00Z").expect("valid timestamp");

    let clock = SourceClock::from_raw(Some("2026-09-20T11:00:00+01:00"));

    assert_eq!(clock, SourceClock::Valid(expected));
}

#[test]
fn malformed_source_time_retains_its_trimmed_spelling() {
    let clock = SourceClock::from_raw(Some(" not-a-time "));

    assert_eq!(clock, SourceClock::Invalid("not-a-time".to_owned()));
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

    assert_eq!(
        complete_empty.completeness(),
        &CollectionCompleteness::Complete
    );
    assert!(complete_empty.payload().is_empty());
}
