//! Checked process-clock reads in archive microseconds.

use std::time::{SystemTime, UNIX_EPOCH};

use forgesync_core::timestamp::UtcTimestamp;

use crate::error::StoreError;
use crate::sql::timestamp_from_sql;

/// Reads the process clock as a checked archive timestamp, truncating sub-microsecond precision.
pub fn now_utc() -> Result<UtcTimestamp, StoreError> {
    timestamp_from_sql(unix_microseconds()?)
}

/// Reads microseconds since the Unix epoch for lease expiry comparisons.
pub fn unix_microseconds() -> Result<i64, StoreError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StoreError::ClockOutOfRange)?;
    i64::try_from(elapsed.as_micros()).map_err(|_| StoreError::ClockOutOfRange)
}
