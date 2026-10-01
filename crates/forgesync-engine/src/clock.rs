//! # Engine wall-clock acquisition
//!
//! [`now_utc`] converts the process wall clock into the domain's UTC microsecond representation.
//! Acquisition, document writes, lease coordination, and derived-analysis operations use the same
//! conversion and failure classification rather than borrowing a helper from another workflow.
//!
//! This is wall time, not a monotonic ordering source. Observation sequences and store reservation
//! rules own durable ordering; runtime timers own elapsed durations. Repeated calls can return the
//! same timestamp or move backwards if the system clock changes.
//!
//! The private module exposes one ordinary public helper to sibling workflows. It reads no
//! configuration and installs no process diagnostics. Pre-epoch or unrepresentable instants fail;
//! sub-microsecond precision is discarded during conversion.

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
