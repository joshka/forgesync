//! The single ordering rule for canonical observations, shared by parent and child application.
//!
//! Source revision wins over arrival order; missing or equal revisions fall back to acquisition
//! sequence, and distinct malformed revisions are ambiguous rather than last-write-wins.

use std::cmp::Ordering;

use forgesync_core::identity::ObservationSequence;
use forgesync_core::observation::SourceClock;

use crate::error::StoreError;

/// Orders canonical observations by provider source clock, then acquisition sequence.
///
/// A valid clock outranks an unusable one; distinct unusable spellings return
/// [`StoreError::AmbiguousObservationClocks`].
pub(crate) fn compare_observation_order(
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
