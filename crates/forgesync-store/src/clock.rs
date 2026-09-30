//! # Checked wall-clock acquisition for archive operations
//!
//! Archive creation and diagnostics use [`now_utc`] for domain timestamps. Lease validation uses
//! [`unix_microseconds`] for comparison with stored expiry coordinates. Both paths share the same
//! process-clock conversion, keeping range and pre-epoch failure policy in one place.
//!
//! These values are wall-clock observations, not monotonic ordering or lease ownership evidence.
//! Sub-microsecond precision is truncated to SQLite's archive unit. Observation sequences and
//! transactional fencing remain responsible for acquisition ordering and writer validity.
//!
//! This private module does not read application configuration or mutate an archive. Callers decide
//! when to acquire time; explicit timestamp inputs remain available on workflow and lease APIs.

use std::time::{SystemTime, UNIX_EPOCH};

use forgesync_core::timestamp::UtcTimestamp;

use crate::error::StoreError;

/// Reads the process clock as a checked archive timestamp, truncating sub-microsecond precision.
///
/// Pre-epoch or overflowing clock values fail before conversion; domain timestamp failures retain
/// the archive timestamp error used by creation and diagnostics.
pub fn now_utc() -> Result<UtcTimestamp, StoreError> {
    let microseconds = unix_microseconds()?;
    UtcTimestamp::from_unix_microseconds(microseconds).map_err(StoreError::InvalidCreatedAt)
}

/// Reads microseconds since Unix epoch for lease comparisons without manufacturing a domain value.
///
/// Returns [`StoreError::ClockOutOfRange`] for pre-epoch time or values beyond signed archive
/// units. This observation grants no fencing capability and is not a monotonic clock.
pub fn unix_microseconds() -> Result<i64, StoreError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StoreError::ClockOutOfRange)?;
    i64::try_from(elapsed.as_micros()).map_err(|_| StoreError::ClockOutOfRange)
}
