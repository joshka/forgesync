//! Archive health and pending work diagnostics.

use std::time::{SystemTime, UNIX_EPOCH};

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::timestamp::UtcTimestamp;
use serde::Serialize;
use sqlx::Row;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::migration::{
    MIGRATOR, current_schema_version, supported_schema_version, validate_migration_history,
};

/// Read-only details about archive compatibility, pending work, and the writer lease.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ArchiveDiagnostics {
    /// Schema history and migrations supported by this binary.
    pub schema: SchemaDiagnostics,
    /// Current or expired archive writer lease.
    pub lease: ArchiveLeaseStatus,
    /// Counts of durable failed and deferred work.
    pub work: WorkDiagnostics,
}

/// Schema version and validated migration-history summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SchemaDiagnostics {
    /// Highest successful migration in the archive.
    pub current_version: i64,
    /// Highest migration included in this binary.
    pub supported_version: i64,
    /// Migrations present in this binary but not applied to the archive.
    pub pending_migrations: Vec<PendingMigration>,
    /// Whether every applied migration checksum matches this binary.
    pub history_valid: bool,
}

/// Migration available for explicit application.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PendingMigration {
    /// Ordered migration version.
    pub version: i64,
    /// Short description from the migration file.
    pub description: String,
}

/// Current lease owner and expiry as observed by a read-only query.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ArchiveLeaseStatus {
    /// Stable owner identity, present while a lease has not been released.
    pub owner_id: Option<String>,
    /// Monotonically increasing fencing token.
    pub fencing_token: u64,
    /// Lease expiry, including the last released or expired timestamp.
    pub expires_at: UtcTimestamp,
    /// True only when an owner exists and expiry is in the future.
    pub held: bool,
}

/// Durable failed, deferred, and unresolved work counts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkDiagnostics {
    /// Jobs whose last recorded status is failed.
    pub failed_jobs: u64,
    /// Jobs whose last recorded status is deferred.
    pub deferred_jobs: u64,
    /// Runs left in progress by an interrupted process.
    pub in_progress_runs: u64,
    /// Unresolved failures grouped by evidence family.
    pub failures_by_family: Vec<FamilyFailureCount>,
    /// Unresolved failures without a selected evidence family.
    pub unassigned_failures: u64,
    /// Total unresolved failure-ledger entries.
    pub unresolved_failures: u64,
}

/// Unresolved failure count for one evidence family.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FamilyFailureCount {
    /// Evidence family with retryable failures.
    pub family: EvidenceFamily,
    /// Number of unresolved failures for this family.
    pub unresolved: u64,
}

impl Archive {
    /// Reads archive diagnostics without acquiring a write lease or changing archive state.
    pub async fn diagnostics(&self) -> Result<ArchiveDiagnostics, StoreError> {
        let current_version = current_schema_version(&self.reader).await?;
        validate_migration_history(&self.reader, current_version).await?;
        let supported_version = supported_schema_version();
        let pending_migrations = MIGRATOR
            .iter()
            .filter(|migration| migration.version > current_version)
            .map(|migration| PendingMigration {
                version: migration.version,
                description: migration.description.to_string(),
            })
            .collect();

        let lease_row = sqlx::query(
            "SELECT owner_id, fencing_token, expires_at_us FROM archive_lease WHERE singleton = 1",
        )
        .fetch_optional(&self.reader)
        .await?;
        let (owner_id, fencing_token, expires_at_us): (Option<String>, i64, i64) = match lease_row {
            Some(row) => (
                row.try_get("owner_id")?,
                row.try_get("fencing_token")?,
                row.try_get("expires_at_us")?,
            ),
            None => (None, 0, 0),
        };
        let expires_at = UtcTimestamp::from_unix_microseconds(expires_at_us)
            .map_err(StoreError::InvalidCreatedAt)?;
        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| StoreError::ClockOutOfRange)?;
        let now_us = i64::try_from(elapsed.as_micros()).map_err(|_| StoreError::ClockOutOfRange)?;
        let now =
            UtcTimestamp::from_unix_microseconds(now_us).map_err(StoreError::InvalidCreatedAt)?;
        let held = owner_id.is_some() && expires_at > now;
        let fencing_token =
            u64::try_from(fencing_token).map_err(|_| StoreError::InvalidStoredCount)?;

        let failed_jobs = count(
            &self.reader,
            "SELECT COUNT(*) FROM jobs WHERE status = 'failed'",
        )
        .await?;
        let deferred_jobs = count(
            &self.reader,
            "SELECT COUNT(*) FROM jobs WHERE status = 'deferred'",
        )
        .await?;
        let in_progress_runs = count(
            &self.reader,
            "SELECT COUNT(*) FROM runs WHERE status = 'in_progress'",
        )
        .await?;
        let mut failures_by_family = Vec::with_capacity(5);
        for (family, name) in [
            (EvidenceFamily::Threads, "threads"),
            (EvidenceFamily::Comments, "comments"),
            (EvidenceFamily::PullRequestMetadata, "pull_request_metadata"),
            (EvidenceFamily::Reviews, "reviews"),
            (EvidenceFamily::ReviewThreads, "review_threads"),
        ] {
            failures_by_family.push(FamilyFailureCount {
                family,
                unresolved: count_bound(
                    &self.reader,
                    "SELECT COUNT(*) FROM failures WHERE resolved_at_us IS NULL AND family = ?",
                    name,
                )
                .await?,
            });
        }
        let unassigned_failures = count(
            &self.reader,
            "SELECT COUNT(*) FROM failures WHERE resolved_at_us IS NULL AND family IS NULL",
        )
        .await?;
        let unresolved_failures = count(
            &self.reader,
            "SELECT COUNT(*) FROM failures WHERE resolved_at_us IS NULL",
        )
        .await?;

        Ok(ArchiveDiagnostics {
            schema: SchemaDiagnostics {
                current_version,
                supported_version,
                pending_migrations,
                history_valid: true,
            },
            lease: ArchiveLeaseStatus {
                owner_id,
                fencing_token,
                expires_at,
                held,
            },
            work: WorkDiagnostics {
                failed_jobs,
                deferred_jobs,
                in_progress_runs,
                failures_by_family,
                unassigned_failures,
                unresolved_failures,
            },
        })
    }
}

async fn count(pool: &sqlx::SqlitePool, sql: &'static str) -> Result<u64, StoreError> {
    let value: i64 = sqlx::query_scalar(sql).fetch_one(pool).await?;
    u64::try_from(value).map_err(|_| StoreError::InvalidStoredCount)
}

async fn count_bound(
    pool: &sqlx::SqlitePool,
    sql: &'static str,
    value: &str,
) -> Result<u64, StoreError> {
    let value: i64 = sqlx::query_scalar(sql).bind(value).fetch_one(pool).await?;
    u64::try_from(value).map_err(|_| StoreError::InvalidStoredCount)
}
