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
///
/// Compare the incoming and currently accepted positions without reading or writing an archive.
/// A newer valid provider revision wins even when its acquisition sequence is lower. Equal valid
/// instants, matching malformed spellings, and two missing clocks use sequence as the tie break.
/// A valid clock outranks an unusable one; distinct unusable spellings return an ambiguity error
/// instead of making canonical content depend on arrival order.
///
/// The caller supplies sequences from the relevant archive. Checked positive values alone do not
/// prove reservation or membership; this function decides relative order, not write authority.
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
///
/// This policy selects complete revision evidence independently of canonical source high water.
/// Different present sequences take precedence over source time; a present sequence outranks a
/// missing one. Equal or absent sequences fall back to source-clock comparison. The ambiguity
/// check runs first so distinct unusable clocks cannot silently become compatible through order.
/// No archive state is inspected or changed, and the result does not authorize persistence.
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
mod tests;
