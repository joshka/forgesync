//! Exclusive archive writer leases.
//!
//! A writer lease coordinates long-running mutations across processes. Acquire and release it
//! around a workflow, but do not keep a SQLite transaction open across provider requests.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forgesync_core::timestamp::UtcTimestamp;
use sqlx::SqliteConnection;
use uuid::Uuid;

use crate::archive::Archive;
use crate::error::StoreError;

/// Opaque fencing identity for one active archive writer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveLeaseToken {
    owner_id: String,
    fencing_token: i64,
}

impl Archive {
    /// Claims the archive write lease for a bounded period.
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

    /// Extends the lease only when this owner and fencing token remain current.
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

    /// Releases a lease if it still belongs to this owner and fencing token.
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

/// Fences a write against the current unexpired archive lease.
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
    if duration.is_zero() {
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
