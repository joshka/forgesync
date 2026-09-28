use forgesync_core::{Failure, ObservationSequence, RepositoryId, UtcTimestamp};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqliteConnection};

use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::{Archive, StoreError};

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
    /// Number of pages whose content was committed before the checkpoint advanced.
    pub pages_completed: u64,
    /// Number of discussion items committed by completed pages.
    pub threads_seen: u64,
    /// Safe provider or archive failure that stopped acquisition, when known.
    pub failure: Option<Failure>,
}

impl Archive {
    /// Starts a new repository enumeration and records its first page before requesting it.
    pub async fn begin_repository_thread_scan(
        &self,
        repository: &RepositoryId,
        sequence: ObservationSequence,
        started_at: UtcTimestamp,
        first_page_url: &str,
    ) -> Result<(), StoreError> {
        self.begin_repository_thread_scan_inner(
            repository,
            sequence,
            started_at,
            first_page_url,
            None,
        )
        .await
    }

    /// Starts an enumeration only while the supplied archive lease remains current.
    pub async fn begin_repository_thread_scan_fenced(
        &self,
        repository: &RepositoryId,
        sequence: ObservationSequence,
        started_at: UtcTimestamp,
        first_page_url: &str,
        token: &ArchiveLeaseToken,
    ) -> Result<(), StoreError> {
        self.begin_repository_thread_scan_inner(
            repository,
            sequence,
            started_at,
            first_page_url,
            Some(token),
        )
        .await
    }

    async fn begin_repository_thread_scan_inner(
        &self,
        repository: &RepositoryId,
        sequence: ObservationSequence,
        started_at: UtcTimestamp,
        first_page_url: &str,
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<(), StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let sequence = to_sql_integer(sequence.get())?;
        let mut transaction = writer.begin().await?;
        if let Some(token) = token {
            require_active_archive_lease(&mut transaction, token).await?;
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

    /// Commits one page checkpoint after all items on that page have been applied.
    pub async fn record_repository_thread_scan_page(
        &self,
        repository: &RepositoryId,
        sequence: ObservationSequence,
        thread_count: u64,
        next_page_url: Option<&str>,
        updated_at: UtcTimestamp,
    ) -> Result<(), StoreError> {
        self.record_repository_thread_scan_page_inner(
            repository,
            sequence,
            thread_count,
            next_page_url,
            updated_at,
            None,
        )
        .await
    }

    /// Commits a page only while the supplied archive lease remains current.
    pub async fn record_repository_thread_scan_page_fenced(
        &self,
        repository: &RepositoryId,
        sequence: ObservationSequence,
        thread_count: u64,
        next_page_url: Option<&str>,
        updated_at: UtcTimestamp,
        token: &ArchiveLeaseToken,
    ) -> Result<(), StoreError> {
        self.record_repository_thread_scan_page_inner(
            repository,
            sequence,
            thread_count,
            next_page_url,
            updated_at,
            Some(token),
        )
        .await
    }

    async fn record_repository_thread_scan_page_inner(
        &self,
        repository: &RepositoryId,
        sequence: ObservationSequence,
        thread_count: u64,
        next_page_url: Option<&str>,
        updated_at: UtcTimestamp,
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<(), StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let sequence = to_sql_integer(sequence.get())?;
        let thread_count = to_sql_integer(thread_count)?;
        let mut transaction = writer.begin().await?;
        if let Some(token) = token {
            require_active_archive_lease(&mut transaction, token).await?;
        }
        let repository_row_id = repository_row_id(&mut transaction, repository).await?;
        let row = sqlx::query(
            "SELECT pages_completed, threads_seen FROM repository_thread_scans WHERE repository_id = ? AND sequence = ? AND status = 'in_progress'",
        )
        .bind(repository_row_id)
        .bind(sequence)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(StoreError::RepositoryThreadScanMissing)?;
        let pages_completed = checked_count(row.try_get("pages_completed")?)?
            .checked_add(1)
            .ok_or(StoreError::IntegerOutOfRange)?;
        let threads_seen = checked_count(row.try_get("threads_seen")?)?
            .checked_add(u64::try_from(thread_count).map_err(|_| StoreError::IntegerOutOfRange)?)
            .ok_or(StoreError::IntegerOutOfRange)?;
        let pages_completed = to_sql_integer(pages_completed)?;
        let threads_seen = to_sql_integer(threads_seen)?;

        sqlx::query(
            "UPDATE repository_thread_scans SET updated_at_us = ?, next_page_url = ?, pages_completed = ?, threads_seen = ? WHERE repository_id = ? AND sequence = ? AND status = 'in_progress'",
        )
        .bind(updated_at.unix_microseconds())
        .bind(next_page_url)
        .bind(pages_completed)
        .bind(threads_seen)
        .bind(repository_row_id)
        .bind(sequence)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Marks an active scan complete or incomplete without advancing its page cursor.
    pub async fn finish_repository_thread_scan(
        &self,
        repository: &RepositoryId,
        sequence: ObservationSequence,
        status: RepositoryThreadScanStatus,
        updated_at: UtcTimestamp,
        failure: Option<&Failure>,
    ) -> Result<(), StoreError> {
        self.finish_repository_thread_scan_inner(
            repository, sequence, status, updated_at, failure, None,
        )
        .await
    }

    /// Finishes a scan only while the supplied archive lease remains current.
    pub async fn finish_repository_thread_scan_fenced(
        &self,
        repository: &RepositoryId,
        sequence: ObservationSequence,
        status: RepositoryThreadScanStatus,
        updated_at: UtcTimestamp,
        failure: Option<&Failure>,
        token: &ArchiveLeaseToken,
    ) -> Result<(), StoreError> {
        self.finish_repository_thread_scan_inner(
            repository,
            sequence,
            status,
            updated_at,
            failure,
            Some(token),
        )
        .await
    }

    async fn finish_repository_thread_scan_inner(
        &self,
        repository: &RepositoryId,
        sequence: ObservationSequence,
        status: RepositoryThreadScanStatus,
        updated_at: UtcTimestamp,
        failure: Option<&Failure>,
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<(), StoreError> {
        if status == RepositoryThreadScanStatus::InProgress {
            return Err(StoreError::InvalidRepositoryThreadScan);
        }
        if status == RepositoryThreadScanStatus::Complete && failure.is_some() {
            return Err(StoreError::InvalidRepositoryThreadScan);
        }

        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let sequence = to_sql_integer(sequence.get())?;
        let mut transaction = writer.begin().await?;
        if let Some(token) = token {
            require_active_archive_lease(&mut transaction, token).await?;
        }
        let row = sqlx::query(
            "SELECT next_page_url FROM repository_thread_scans WHERE repository_id = (SELECT id FROM repositories WHERE host = ? AND provider_id = ?) AND sequence = ? AND status = 'in_progress'",
        )
        .bind(repository.host().as_str())
        .bind(repository.provider_id().as_str())
        .bind(sequence)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(StoreError::RepositoryThreadScanMissing)?;
        let next_page_url: Option<String> = row.try_get("next_page_url")?;
        if status == RepositoryThreadScanStatus::Complete && next_page_url.is_some() {
            return Err(StoreError::InvalidRepositoryThreadScan);
        }
        let status_name = match status {
            RepositoryThreadScanStatus::InProgress => unreachable!(),
            RepositoryThreadScanStatus::Incomplete => "incomplete",
            RepositoryThreadScanStatus::Complete => "complete",
        };
        let failure_json = failure.map(serde_json::to_string).transpose()?;
        let result = sqlx::query(
            "UPDATE repository_thread_scans SET status = ?, updated_at_us = ?, next_page_url = CASE WHEN ? = 'complete' THEN NULL ELSE next_page_url END, failure_json = ? WHERE repository_id = (SELECT id FROM repositories WHERE host = ? AND provider_id = ?) AND sequence = ? AND status = 'in_progress'",
        )
        .bind(status_name)
        .bind(updated_at.unix_microseconds())
        .bind(status_name)
        .bind(failure_json)
        .bind(repository.host().as_str())
        .bind(repository.provider_id().as_str())
        .bind(sequence)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(StoreError::RepositoryThreadScanMissing);
        }
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

fn to_sql_integer(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::IntegerOutOfRange)
}

fn checked_count(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::InvalidStoredCount)
}

fn checked_positive_sequence(value: i64) -> Result<ObservationSequence, StoreError> {
    let value = u64::try_from(value).map_err(|_| StoreError::InvalidStoredSequence)?;
    ObservationSequence::new(value).map_err(|_| StoreError::InvalidStoredSequence)
}

fn decode_timestamp(value: i64) -> Result<UtcTimestamp, StoreError> {
    UtcTimestamp::from_unix_microseconds(value).map_err(StoreError::InvalidCreatedAt)
}

fn decode_status(value: String) -> Result<RepositoryThreadScanStatus, StoreError> {
    match value.as_str() {
        "in_progress" => Ok(RepositoryThreadScanStatus::InProgress),
        "incomplete" => Ok(RepositoryThreadScanStatus::Incomplete),
        "complete" => Ok(RepositoryThreadScanStatus::Complete),
        _ => Err(StoreError::InvalidRepositoryThreadScan),
    }
}
