//! Fixtures shared by the store integration suites.

#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;
use forgesync_store::leases::ArchiveLeaseToken;

/// Acquires a writer lease valid at the current process clock, as fenced writes require.
pub async fn lease(archive: &Archive) -> ArchiveLeaseToken {
    archive
        .acquire_archive_lease(now(), Duration::from_secs(600))
        .await
        .expect("acquire archive lease")
}

/// Reads the process clock; fenced writes compare lease expiry against it.
pub fn now() -> UtcTimestamp {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch");
    UtcTimestamp::from_unix_microseconds(i64::try_from(elapsed.as_micros()).expect("clock range"))
        .expect("current timestamp")
}
