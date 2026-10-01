//! Wall-clock acquisition in the domain's UTC microsecond representation.
//!
//! This is wall time, not an ordering source: observation sequences own durable order, and calls
//! can repeat or move backwards if the system clock changes.

use std::time::{SystemTime, UNIX_EPOCH};

use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::error::StoreError;

use crate::error::EngineError;

/// Reads wall time as a validated UTC instant, truncating sub-microsecond precision.
pub fn now_utc() -> Result<UtcTimestamp, EngineError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StoreError::ClockOutOfRange)?;
    let microseconds =
        i64::try_from(elapsed.as_micros()).map_err(|_| StoreError::ClockOutOfRange)?;
    UtcTimestamp::from_unix_microseconds(microseconds)
        .map_err(StoreError::InvalidTimestamp)
        .map_err(Into::into)
}
