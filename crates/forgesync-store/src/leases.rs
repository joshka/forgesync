//! Durable archive writer leases.
//!
//! A lease is a concurrency boundary, not a transaction across network I/O. A token is neither an
//! RAII guard nor proof of current ownership: every guarded write rechecks owner, fence, and expiry
//! against the process clock inside its own transaction.

use std::time::Duration;

use forgesync_core::timestamp::UtcTimestamp;
use sqlx::SqliteConnection;
use uuid::Uuid;

use crate::archive::Archive;
use crate::clock::unix_microseconds;
use crate::error::StoreError;
use crate::sql::timestamp_from_sql;

/// Opaque owner and fencing identity returned by successful lease acquisition.
///
/// Dropping it does not release the lease; use [`Archive::release_archive_lease`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveLeaseToken {
    /// Fresh acquisition identity, compared alongside the durable fencing number.
    owner_id: String,
    /// Positive SQLite fencing number that distinguishes successive acquisitions.
    fencing_token: i64,
}

impl Archive {
    /// Claims the singleton writer lease when released or expired at `now`, expiring at
    /// `now + duration`.
    ///
    /// `now` is caller-supplied: a historical timestamp yields a claim already expired for guarded
    /// writes. Returns [`StoreError::ArchiveLeaseHeld`] when another owner is unexpired at `now`.
    pub async fn acquire_archive_lease(
        &self,
        now: UtcTimestamp,
        duration: Duration,
    ) -> Result<ArchiveLeaseToken, StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let duration_us = duration_microseconds(duration)?;
        let expires_at_us = now
            .unix_microseconds()
            .checked_add(duration_us)
            .ok_or(StoreError::InvalidArchiveLeaseDuration)?;
        let owner_id = Uuid::new_v4().to_string();
        let fencing_token: Option<i64> = sqlx::query_scalar(
            "INSERT INTO archive_lease (singleton, owner_id, fencing_token, expires_at_us, updated_at_us) VALUES (1, ?, 1, ?, ?) ON CONFLICT (singleton) DO UPDATE SET owner_id = excluded.owner_id, fencing_token = archive_lease.fencing_token + 1, expires_at_us = excluded.expires_at_us, updated_at_us = excluded.updated_at_us WHERE archive_lease.owner_id IS NULL OR archive_lease.expires_at_us <= excluded.updated_at_us RETURNING fencing_token",
        )
        .bind(&owner_id)
        .bind(expires_at_us)
        .bind(now.unix_microseconds())
        .fetch_optional(writer)
        .await?;
        let fencing_token = fencing_token.ok_or(StoreError::ArchiveLeaseHeld)?;
        Ok(ArchiveLeaseToken {
            owner_id,
            fencing_token,
        })
    }

    /// Replaces expiry with `now + duration` for this current, unexpired owner/fence.
    ///
    /// Does not revive an expired or superseded lease ([`StoreError::ArchiveLeaseLost`]).
    pub async fn heartbeat_archive_lease(
        &self,
        token: &ArchiveLeaseToken,
        now: UtcTimestamp,
        duration: Duration,
    ) -> Result<UtcTimestamp, StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let duration_us = duration_microseconds(duration)?;
        let expires_at_us = now
            .unix_microseconds()
            .checked_add(duration_us)
            .ok_or(StoreError::InvalidArchiveLeaseDuration)?;
        let result = sqlx::query(
            "UPDATE archive_lease SET expires_at_us = ?, updated_at_us = ? WHERE singleton = 1 AND owner_id = ? AND fencing_token = ? AND expires_at_us > ?",
        )
        .bind(expires_at_us)
        .bind(now.unix_microseconds())
        .bind(&token.owner_id)
        .bind(token.fencing_token)
        .bind(now.unix_microseconds())
        .execute(writer)
        .await?;
        if result.rows_affected() != 1 {
            return Err(StoreError::ArchiveLeaseLost);
        }
        timestamp_from_sql(expires_at_us)
    }

    /// Clears this owner/fence without releasing a successor's lease.
    ///
    /// Returns `false` when already released or superseded.
    pub async fn release_archive_lease(
        &self,
        token: &ArchiveLeaseToken,
        now: UtcTimestamp,
    ) -> Result<bool, StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let result = sqlx::query(
            "UPDATE archive_lease SET owner_id = NULL, expires_at_us = ?, updated_at_us = ? WHERE singleton = 1 AND owner_id = ? AND fencing_token = ?",
        )
        .bind(now.unix_microseconds())
        .bind(now.unix_microseconds())
        .bind(&token.owner_id)
        .bind(token.fencing_token)
        .execute(writer)
        .await?;
        Ok(result.rows_affected() == 1)
    }
}

/// Fences a write against the current owner, fencing number, and process-clock expiry.
///
/// Call inside the mutation's transaction before changing guarded state.
pub(crate) async fn require_active_archive_lease(
    connection: &mut SqliteConnection,
    token: &ArchiveLeaseToken,
) -> Result<(), StoreError> {
    let now = unix_microseconds()?;
    let active: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM archive_lease WHERE singleton = 1 AND owner_id = ? AND fencing_token = ? AND expires_at_us > ?)",
    )
    .bind(&token.owner_id)
    .bind(token.fencing_token)
    .bind(now)
    .fetch_one(connection)
    .await?;
    if active == 1 {
        Ok(())
    } else {
        Err(StoreError::ArchiveLeaseLost)
    }
}

/// Checks a lease duration before converting it to archive units.
fn duration_microseconds(duration: Duration) -> Result<i64, StoreError> {
    if duration.as_micros() == 0 {
        return Err(StoreError::InvalidArchiveLeaseDuration);
    }
    i64::try_from(duration.as_micros()).map_err(|_| StoreError::InvalidArchiveLeaseDuration)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::error::StoreError;
    use crate::leases::duration_microseconds;

    #[test]
    fn zero_duration_is_rejected() {
        let result = duration_microseconds(Duration::ZERO);
        assert!(matches!(
            result,
            Err(StoreError::InvalidArchiveLeaseDuration)
        ));
    }

    #[test]
    fn submicrosecond_duration_is_rejected() {
        let result = duration_microseconds(Duration::from_nanos(999));
        assert!(matches!(
            result,
            Err(StoreError::InvalidArchiveLeaseDuration)
        ));
    }

    #[test]
    fn whole_microseconds_are_retained_and_finer_precision_is_truncated() {
        let result = duration_microseconds(Duration::from_nanos(1_999));
        assert_eq!(result.expect("representable duration"), 1);
    }

    #[test]
    fn duration_outside_signed_storage_range_is_rejected() {
        let result = duration_microseconds(Duration::MAX);
        assert!(matches!(
            result,
            Err(StoreError::InvalidArchiveLeaseDuration)
        ));
    }
}
