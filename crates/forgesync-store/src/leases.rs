//! # Serialize archive work with durable leases
//!
//! `ArchiveLeaseToken` identifies an acquired archive writer lease. `Archive` methods
//! acquire, inspect, and finish leases so concurrent local processes do not both commit the same
//! coordinated operation.
//!
//! Callers pass the token into guarded writes; the guard checks that it still names an active
//! lease. A lease is a concurrency boundary, not a database transaction across network I/O. The
//! engine performs provider calls outside transactions and finishes or records failure afterward.
//!
//! [`Archive::acquire_archive_lease`] claims the singleton writer slot when it is released or
//! expired at the supplied time. Each acquisition creates a fresh owner and advances the durable
//! fencing number. [`Archive::heartbeat_archive_lease`] updates the expiry of that exact owner;
//! [`Archive::release_archive_lease`] clears it only if no successor has acquired the slot.
//!
//! A token is neither an RAII guard nor proof that the lease is still active. Cloning or dropping
//! it has no database effect. Guarded mutations compare owner, fence, and expiry inside their own
//! transaction against the current process clock. Acquisition and heartbeat accept an explicit
//! clock for workflow coordination; callers must supply a current timestamp if they expect the
//! resulting token to authorize writes now.
//!
//! Durations are stored as whole microseconds. Values below one microsecond are rejected rather
//! than creating an already-expired lease; finer precision on longer values is truncated. No
//! automatic heartbeat or release runs in this module. The workflow must maintain its lease and
//! release it on completion, while expiry allows a later owner to recover abandoned work.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forgesync_core::timestamp::UtcTimestamp;
use sqlx::SqliteConnection;
use uuid::Uuid;

use crate::archive::Archive;
use crate::error::StoreError;

/// Opaque owner and fencing identity returned by successful lease acquisition.
///
/// This value records a claim, not its continued validity. Every guarded write rechecks it against
/// durable state and current expiry. Clones share the same claim; dropping the last clone neither
/// releases the lease nor stops work. Use [`Archive::release_archive_lease`] explicitly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveLeaseToken {
    /// Fresh acquisition identity, compared alongside the durable fencing number.
    owner_id: String,
    /// Positive SQLite fencing number that distinguishes successive acquisitions.
    fencing_token: i64,
}

impl Archive {
    /// Claims the singleton writer lease when released or expired at `now`.
    ///
    /// Stores expiry as `now + duration` and returns a fresh owner/fence pair. `now` is supplied
    /// by the caller rather than sampled here: a historical timestamp can create a claim that is
    /// already expired for guarded writes. No heartbeat task is started.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::ReadOnlyArchive`] without a writer, [`StoreError::ArchiveLeaseHeld`]
    /// when the persisted owner is unexpired at `now`, or
    /// [`StoreError::InvalidArchiveLeaseDuration`] for a duration below one microsecond,
    /// outside the signed storage range, or overflowing the expiry addition. Invalid returned
    /// fencing state and database errors are propagated.
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
        if fencing_token <= 0 {
            return Err(StoreError::InvalidStoredSequence);
        }
        Ok(ArchiveLeaseToken {
            owner_id,
            fencing_token,
        })
    }

    /// Replaces expiry with `now + duration` for this current, unexpired owner/fence.
    ///
    /// Validity is evaluated at the caller-supplied `now`. This does not revive a lease already
    /// expired at that time or one acquired by a successor. The new expiry is computed from `now`,
    /// not the previous expiry, so a shorter duration can shorten the remaining lease.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::ArchiveLeaseLost`] when the owner, fence, or expiry check fails.
    /// Read-only, duration, arithmetic, and database failures follow the same rules as
    /// [`Archive::acquire_archive_lease`]. Returns the stored expiry as a checked timestamp.
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
        UtcTimestamp::from_unix_microseconds(expires_at_us).map_err(StoreError::InvalidCreatedAt)
    }

    /// Clears this owner/fence without releasing a successor's lease.
    ///
    /// Returns `true` when the row was released, including when this owner's lease has already
    /// expired. Returns `false` after release or successor acquisition. The supplied `now` becomes
    /// the diagnostic expiry/update time; the fencing number remains available for the next claim.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::ReadOnlyArchive`] without a writer and propagates database failures.
    /// A stale token is an ordinary `false` result rather than [`StoreError::ArchiveLeaseLost`].
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

/// Fences a write against the current owner, acquisition number, and process-clock expiry.
///
/// Call inside the mutation's transaction before changing guarded state. Possession of a token
/// alone is insufficient. The clock is sampled here rather than taken from an observation or the
/// earlier acquisition request. Clock conversion, database failure, or lost ownership rejects the
/// write. This implementation helper stays crate-visible because its raw connection must not
/// become part of the public archive API.
pub(crate) async fn require_active_archive_lease(
    connection: &mut SqliteConnection,
    token: &ArchiveLeaseToken,
) -> Result<(), StoreError> {
    let now = current_unix_microseconds()?;
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

/// Reads a checked local clock value for lease expiry.
fn current_unix_microseconds() -> Result<i64, StoreError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StoreError::ClockOutOfRange)?;
    i64::try_from(elapsed.as_micros()).map_err(|_| StoreError::ClockOutOfRange)
}

#[cfg(test)]
mod tests {
    //! Lease duration conversion boundaries.
    //!
    //! These cases protect the units used by both acquisition and heartbeat. In particular a
    //! nonzero nanosecond duration must not silently become an immediate-expiry zero in SQLite.
    //! Ownership and concurrent writer behavior are exercised by the archive lease integration
    //! suite; these tests keep conversion setup and failure expectations local.

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
