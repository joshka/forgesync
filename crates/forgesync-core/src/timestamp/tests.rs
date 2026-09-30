//! # UTC representation and archive precision contracts
//!
//! These cases cover equal instants with different offsets, JSON representation, invalid input,
//! signed archive range, and microsecond truncation. Precision cases name both sides of Unix epoch,
//! where division toward zero matters to the retained instant.
//!
//! Parsing source text and reconstructing stored integers exercise separate boundaries. Tests use
//! fixed instants rather than a process clock so expectations remain deterministic. Archive clock
//! acquisition and lease validity belong to store tests; this suite only validates the value and
//! its representation. Add a named boundary case when those conversion contracts change.

use serde_json::json;

use crate::timestamp::{TimestampError, UtcTimestamp};

#[test]
fn timestamps_compare_as_instants_and_serialize_as_utc() {
    let local = UtcTimestamp::parse("2026-09-20T11:00:00+01:00").expect("valid timestamp");
    let utc = UtcTimestamp::parse("2026-09-20T10:00:00Z").expect("valid timestamp");
    assert_eq!(local, utc);
    assert_eq!(
        local.format_rfc3339().expect("format"),
        "2026-09-20T10:00:00Z"
    );
    assert_eq!(local.unix_microseconds(), utc.unix_microseconds());

    let encoded = serde_json::to_value(local).expect("serialize timestamp");
    assert_eq!(encoded, json!("2026-09-20T10:00:00Z"));
    let decoded: UtcTimestamp = serde_json::from_value(encoded).expect("deserialize timestamp");
    assert_eq!(decoded, utc);
}

#[test]
fn invalid_timestamp_inputs_fail_clearly() {
    assert_eq!(
        UtcTimestamp::parse("not-a-time"),
        Err(TimestampError::InvalidRfc3339)
    );
    let decoded: Result<UtcTimestamp, _> = serde_json::from_value(json!("2026-99-40T00:00:00Z"));
    assert!(decoded.is_err(), "invalid serialized time must be rejected");
}

#[test]
fn pre_epoch_archive_microseconds_round_trip() {
    let timestamp = UtcTimestamp::parse("1969-12-31T23:59:59.123456Z").expect("valid time");
    let micros = timestamp.unix_microseconds();

    assert_eq!(UtcTimestamp::from_unix_microseconds(micros), Ok(timestamp));
}

#[rstest::rstest]
#[case::maximum(i64::MAX)]
#[case::minimum(i64::MIN)]
fn unsupported_archive_instants_are_rejected(#[case] micros: i64) {
    assert_eq!(
        UtcTimestamp::from_unix_microseconds(micros),
        Err(TimestampError::OutOfRange)
    );
}

#[rstest::rstest]
#[case::after_epoch("1970-01-01T00:00:00.000001999Z", 1)]
#[case::before_epoch("1969-12-31T23:59:59.999998001Z", -1)]
#[case::near_epoch("1969-12-31T23:59:59.999999999Z", 0)]
fn sub_microsecond_precision_truncates_toward_epoch(
    #[case] source: &str,
    #[case] expected_micros: i64,
) {
    let timestamp = UtcTimestamp::parse(source).expect("valid precise timestamp");

    assert_eq!(timestamp.unix_microseconds(), expected_micros);
}

#[test]
fn formatting_does_not_restore_discarded_precision() {
    let precise = UtcTimestamp::parse("2026-09-20T10:00:00.123456789Z").expect("timestamp");
    let stored = UtcTimestamp::from_unix_microseconds(precise.unix_microseconds())
        .expect("microsecond timestamp");

    assert_eq!(
        stored.format_rfc3339().expect("format"),
        "2026-09-20T10:00:00.123456Z"
    );
}
