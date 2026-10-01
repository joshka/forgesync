//! Repository thread-scan progress, recorded separately from thread and child-family coverage.
//!
//! Page checkpoints are written after the page's observations commit, in a separate transaction,
//! so a failed checkpoint never undoes discussion writes and replay relies on observation ordering.

use forgesync_core::coverage::Failure;
use forgesync_core::identity::{ObservationSequence, RepositoryId};
use forgesync_core::timestamp::UtcTimestamp;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqliteConnection};

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};

/// Durable state of one repository thread enumeration.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryThreadScanStatus {
    /// Pages are still being fetched and committed.
    InProgress,
    /// Acquisition stopped before every page was committed.
    Incomplete,
    /// Every page in the enumeration was committed.
    Complete,
}

/// Latest durable page checkpoint for one repository thread enumeration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RepositoryThreadScan {
    /// Stable host-qualified repository identity.
    pub repository_id: RepositoryId,
    /// Acquisition sequence reserved before network access began.
    pub sequence: ObservationSequence,
    /// Current completion state.
    pub status: RepositoryThreadScanStatus,
    /// Time when this scan began.
    pub started_at: UtcTimestamp,
    /// Time when the latest page checkpoint or final state was stored.
    pub updated_at: UtcTimestamp,
    /// URL of the next page to request, or the first page when no page committed yet.
    pub next_page_url: Option<String>,
    /// Number of successfully recorded page checkpoints, including empty terminal pages.
    pub pages_completed: u64,
    /// Caller-reported discussion count accumulated across recorded pages.
    ///
    /// Observation replay or ordering can skip a content replacement, so this is not a count of
    /// newly inserted or changed discussions. Page publication does not verify this number.
    pub threads_seen: u64,
    /// Safe provider or archive failure that stopped acquisition, when known.
    pub failure: Option<Failure>,
}

impl Archive {
    /// Starts a new repository enumeration and records its first page before requesting it.
    ///
    /// The sequence must already be reserved and newer than the repository's stored scan. Success
    /// resets counters and the cursor; it does not delete discussions. With `lease`, the fence is
    /// checked inside the same transaction.
    pub async fn begin_repository_thread_scan(
        &self,
        repository: &RepositoryId,
        sequence: ObservationSequence,
        started_at: UtcTimestamp,
        first_page_url: &str,
        lease: Option<&ArchiveLeaseToken>,
    ) -> Result<(), StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let sequence = to_sql_integer(sequence.get())?;
        let mut transaction = writer.begin().await?;
        if let Some(lease) = lease {
            require_active_archive_lease(&mut transaction, lease).await?;
        }
        let repository_row_id = repository_row_id(&mut transaction, repository).await?;
        let current_sequence: Option<i64> = sqlx::query_scalar(
            "SELECT sequence FROM repository_thread_scans WHERE repository_id = ?",
        )
        .bind(repository_row_id)
        .fetch_optional(&mut *transaction)
        .await?;
        if current_sequence.is_some_and(|current| current >= sequence) {
            return Err(StoreError::StaleRepositoryThreadScan);
        }

        sqlx::query(
            "INSERT INTO repository_thread_scans (repository_id, sequence, status, started_at_us, updated_at_us, next_page_url, pages_completed, threads_seen, failure_json) VALUES (?, ?, 'in_progress', ?, ?, ?, 0, 0, NULL) ON CONFLICT (repository_id) DO UPDATE SET sequence = excluded.sequence, status = 'in_progress', started_at_us = excluded.started_at_us, updated_at_us = excluded.updated_at_us, next_page_url = excluded.next_page_url, pages_completed = 0, threads_seen = 0, failure_json = NULL",
        )
        .bind(repository_row_id)
        .bind(sequence)
        .bind(started_at.unix_microseconds())
        .bind(started_at.unix_microseconds())
        .bind(first_page_url)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Records one page checkpoint after the caller has committed every observation on that page.
    ///
    /// Every call increments the page count and adds `thread_count`, so this is not idempotent.
    /// `next_page_url` becomes the resume cursor; `None` records terminal pagination but does not
    /// finish the scan. Earlier discussion writes are not undone if this checkpoint fails.
    pub async fn record_repository_thread_scan_page(
        &self,
        repository: &RepositoryId,
        sequence: ObservationSequence,
        thread_count: u64,
        next_page_url: Option<&str>,
        updated_at: UtcTimestamp,
        lease: Option<&ArchiveLeaseToken>,
    ) -> Result<(), StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let sequence = to_sql_integer(sequence.get())?;
        let thread_count = to_sql_integer(thread_count)?;
        let mut transaction = writer.begin().await?;
        if let Some(lease) = lease {
            require_active_archive_lease(&mut transaction, lease).await?;
        }
        let repository_row_id = repository_row_id(&mut transaction, repository).await?;
        let result = sqlx::query(
            "UPDATE repository_thread_scans SET updated_at_us = ?, next_page_url = ?, pages_completed = pages_completed + 1, threads_seen = threads_seen + ? WHERE repository_id = ? AND sequence = ? AND status = 'in_progress'",
        )
        .bind(updated_at.unix_microseconds())
        .bind(next_page_url)
        .bind(thread_count)
        .bind(repository_row_id)
        .bind(sequence)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(StoreError::RepositoryThreadScanMissing);
        }
        transaction.commit().await?;
        Ok(())
    }

    /// Marks an active scan complete or incomplete without advancing its page cursor.
    ///
    /// Complete coverage requires the terminal page to have cleared the stored cursor and cannot
    /// carry a failure. Incomplete coverage keeps the cursor and may carry a safe failure.
    pub async fn finish_repository_thread_scan(
        &self,
        repository: &RepositoryId,
        sequence: ObservationSequence,
        status: RepositoryThreadScanStatus,
        updated_at: UtcTimestamp,
        failure: Option<&Failure>,
        lease: Option<&ArchiveLeaseToken>,
    ) -> Result<(), StoreError> {
        let status = terminal_status_name(status, failure)?;
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let sequence = to_sql_integer(sequence.get())?;
        let failure_json = failure.map(serde_json::to_string).transpose()?;
        let mut transaction = writer.begin().await?;
        if let Some(lease) = lease {
            require_active_archive_lease(&mut transaction, lease).await?;
        }
        let repository_row_id = repository_row_id(&mut transaction, repository).await?;
        let next_page_url: Option<Option<String>> = sqlx::query_scalar(
            "SELECT next_page_url FROM repository_thread_scans WHERE repository_id = ? AND sequence = ? AND status = 'in_progress'",
        )
        .bind(repository_row_id)
        .bind(sequence)
        .fetch_optional(&mut *transaction)
        .await?;
        let next_page_url = next_page_url.ok_or(StoreError::RepositoryThreadScanMissing)?;
        if status == "complete" && next_page_url.is_some() {
            return Err(StoreError::InvalidRepositoryThreadScan);
        }
        sqlx::query(
            "UPDATE repository_thread_scans SET status = ?, updated_at_us = ?, failure_json = ? WHERE repository_id = ? AND sequence = ?",
        )
        .bind(status)
        .bind(updated_at.unix_microseconds())
        .bind(failure_json)
        .bind(repository_row_id)
        .bind(sequence)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Returns the latest scan record for a repository, if one has started.
    pub async fn repository_thread_scan(
        &self,
        repository: &RepositoryId,
    ) -> Result<Option<RepositoryThreadScan>, StoreError> {
        let row = sqlx::query(
            "SELECT sequence, status, started_at_us, updated_at_us, next_page_url, pages_completed, threads_seen, failure_json FROM repository_thread_scans WHERE repository_id = (SELECT id FROM repositories WHERE host = ? AND provider_id = ?)",
        )
        .bind(repository.host().as_str())
        .bind(repository.provider_id().as_str())
        .fetch_optional(&self.reader)
        .await?;
        row.map(|row| {
            let sequence = checked_positive_sequence(row.try_get("sequence")?)?;
            let status = decode_status(row.try_get("status")?)?;
            let started_at = decode_timestamp(row.try_get("started_at_us")?)?;
            let updated_at = decode_timestamp(row.try_get("updated_at_us")?)?;
            let pages_completed = checked_count(row.try_get("pages_completed")?)?;
            let threads_seen = checked_count(row.try_get("threads_seen")?)?;
            let failure_json: Option<String> = row.try_get("failure_json")?;
            let failure = failure_json
                .map(|json| serde_json::from_str(&json))
                .transpose()?;
            Ok(RepositoryThreadScan {
                repository_id: repository.clone(),
                sequence,
                status,
                started_at,
                updated_at,
                next_page_url: row.try_get("next_page_url")?,
                pages_completed,
                threads_seen,
                failure,
            })
        })
        .transpose()
    }
}

/// Resolves the repository key used by a durable scan.
async fn repository_row_id(
    connection: &mut SqliteConnection,
    repository: &RepositoryId,
) -> Result<i64, StoreError> {
    sqlx::query_scalar("SELECT id FROM repositories WHERE host = ? AND provider_id = ?")
        .bind(repository.host().as_str())
        .bind(repository.provider_id().as_str())
        .fetch_optional(connection)
        .await?
        .ok_or(StoreError::RepositoryMissing)
}

/// Checks a provider count before binding it to SQLite.
fn to_sql_integer(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::IntegerOutOfRange)
}

/// Rejects negative or overflowing stored scan counts.
fn checked_count(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::InvalidStoredCount)
}

/// Rejects an invalid stored observation sequence.
fn checked_positive_sequence(value: i64) -> Result<ObservationSequence, StoreError> {
    let value = u64::try_from(value).map_err(|_| StoreError::InvalidStoredSequence)?;
    ObservationSequence::new(value).map_err(|_| StoreError::InvalidStoredSequence)
}

/// Converts a stored scan timestamp to checked UTC time.
fn decode_timestamp(value: i64) -> Result<UtcTimestamp, StoreError> {
    UtcTimestamp::from_unix_microseconds(value).map_err(StoreError::InvalidCreatedAt)
}

/// Validates a terminal status/failure pair: complete coverage cannot carry a failure.
fn terminal_status_name(
    status: RepositoryThreadScanStatus,
    failure: Option<&Failure>,
) -> Result<&'static str, StoreError> {
    match (status, failure) {
        (RepositoryThreadScanStatus::Complete, None) => Ok("complete"),
        (RepositoryThreadScanStatus::Incomplete, _) => Ok("incomplete"),
        _ => Err(StoreError::InvalidRepositoryThreadScan),
    }
}

/// Rejects scan status labels unknown to this binary.
fn decode_status(value: String) -> Result<RepositoryThreadScanStatus, StoreError> {
    match value.as_str() {
        "in_progress" => Ok(RepositoryThreadScanStatus::InProgress),
        "incomplete" => Ok(RepositoryThreadScanStatus::Incomplete),
        "complete" => Ok(RepositoryThreadScanStatus::Complete),
        _ => Err(StoreError::InvalidRepositoryThreadScan),
    }
}

#[cfg(test)]
mod tests {
    use forgesync_core::coverage::{Failure, FailureKind};

    use super::{RepositoryThreadScanStatus, terminal_status_name};
    use crate::error::StoreError;

    #[test]
    fn terminal_status_rejects_active_scans_and_failed_completion() {
        let failure = Failure {
            kind: FailureKind::Network,
            message: "request failed".to_owned(),
        };
        assert!(matches!(
            terminal_status_name(RepositoryThreadScanStatus::InProgress, None),
            Err(StoreError::InvalidRepositoryThreadScan)
        ));
        assert!(matches!(
            terminal_status_name(RepositoryThreadScanStatus::Complete, Some(&failure)),
            Err(StoreError::InvalidRepositoryThreadScan)
        ));
        assert_eq!(
            terminal_status_name(RepositoryThreadScanStatus::Complete, None).ok(),
            Some("complete")
        );
        assert_eq!(
            terminal_status_name(RepositoryThreadScanStatus::Incomplete, None).ok(),
            Some("incomplete")
        );
    }
}
