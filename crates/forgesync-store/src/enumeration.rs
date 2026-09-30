//! # Record repository thread-scan coverage
//!
//! `RepositoryThreadScan` and its status report what a repository-wide enumeration actually
//! covered. An engine enumeration run writes this evidence after visiting provider pages; later
//! work can tell complete scans from interrupted ones.
//!
//! This is distinct from a thread observation. Knowing that a repository was scanned does not
//! imply every child resource family of every thread is complete. Store these scopes independently
//! so status and retry paths describe the work that really happened.
//!
//! The private `completion` module validates terminal state and performs the active-generation
//! cursor guard and terminal write. Archive methods retain transaction and lease ownership.
//! Completion requires a cleared next-page cursor; interruption preserves the last checkpoint.
//!
//! Page recording stores caller-reported progress after content application. It does not write
//! discussion observations or inspect provider responses to prove counts or terminal pagination.
//! The engine processes every page observation before advancing this cursor. Item writes and the
//! checkpoint transaction are separate, so replay after failure must use observation ordering.
//! Page recording itself increments counters on every call and is not an idempotent replay API.

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

mod completion;

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

    /// Reserves scan state before fetching repository pages.
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

    /// Records one page checkpoint after the caller has processed every observation on that page.
    ///
    /// `sequence` selects the active scan; `thread_count` increments its accumulated item count.
    /// Every successful call increments the page count, even for an empty page. `next_page_url`
    /// becomes the resume cursor; `None` records terminal pagination but does not finish the scan.
    /// `updated_at` is the local progress time, independent of provider revision timestamps.
    ///
    /// This operation does not apply or verify discussion content, nor count newly changed threads.
    /// Its own counters/cursor commit together, separately from earlier item writes. Repeating
    /// a successful call increments the counters again; callers must not treat page publication
    /// as idempotent. Use the fenced variant when the workflow holds an archive lease.
    ///
    /// # Errors
    ///
    /// Read-only archives, unknown repositories, missing/superseded/non-active scan generations,
    /// invalid stored counts, integer overflow, and database failures are returned. A failed
    /// checkpoint transaction leaves the previous counters and cursor intact, but cannot undo
    /// discussion observations the caller committed earlier.
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

    /// Records a page checkpoint while the supplied archive lease remains current.
    ///
    /// Counter, cursor, timestamp, and replay semantics match
    /// [`Self::record_repository_thread_scan_page`]. The token is checked inside the checkpoint
    /// transaction before writing, so possession of an expired or superseded token is insufficient.
    /// Lease failure rolls back this checkpoint without rolling back earlier discussion writes.
    /// This operation performs no provider I/O or content application.
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

    /// Advances caller-reported counters and cursor in one checkpoint transaction.
    ///
    /// Discussion writes have already happened outside this operation. A supplied fence is checked
    /// in the same transaction as active-generation lookup and progress update; counters are
    /// checked before SQL conversion and commit.
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
    ///
    /// Complete coverage requires the terminal page to have cleared the stored next-page cursor.
    /// Incomplete coverage retains that cursor and may carry a safe failure diagnostic; passing
    /// no failure represents interruption or another incomplete attempt without provider evidence.
    ///
    /// # Errors
    ///
    /// Rejects active status, failure-bearing completion, and completion with a pending cursor.
    /// A missing or superseded active generation returns `RepositoryThreadScanMissing`. Validation
    /// of the status/failure pair precedes writable-archive checks; mutations commit atomically.
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

    /// Finishes a scan while the supplied archive lease remains current.
    ///
    /// Terminal-state and pending-cursor rules match [`Self::finish_repository_thread_scan`].
    /// The fence check shares the terminal-write transaction. Failure preserves the current scan
    /// state and page cursor; it does not undo observations already committed during enumeration.
    /// Completion does not itself publish a closed-sweep watermark or prove child-family coverage.
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

    /// Marks a scan complete only after all selected pages are committed.
    async fn finish_repository_thread_scan_inner(
        &self,
        repository: &RepositoryId,
        sequence: ObservationSequence,
        status: RepositoryThreadScanStatus,
        updated_at: UtcTimestamp,
        failure: Option<&Failure>,
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<(), StoreError> {
        let completion = completion::ScanCompletion::new(status, failure)?;

        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let sequence = to_sql_integer(sequence.get())?;
        let mut transaction = writer.begin().await?;
        if let Some(token) = token {
            require_active_archive_lease(&mut transaction, token).await?;
        }
        completion
            .validate_cursor(&mut transaction, repository, sequence)
            .await?;
        completion
            .write(&mut transaction, repository, sequence, updated_at)
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

/// Rejects scan status labels unknown to this binary.
fn decode_status(value: String) -> Result<RepositoryThreadScanStatus, StoreError> {
    match value.as_str() {
        "in_progress" => Ok(RepositoryThreadScanStatus::InProgress),
        "incomplete" => Ok(RepositoryThreadScanStatus::Incomplete),
        "complete" => Ok(RepositoryThreadScanStatus::Complete),
        _ => Err(StoreError::InvalidRepositoryThreadScan),
    }
}
