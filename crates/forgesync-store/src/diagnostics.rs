//! # Inspect archive state without repairing it
//!
//! `ArchiveDiagnostics` groups schema, lease, and work information for operator-facing status.
//! `SchemaDiagnostics` and `PendingMigration` identify database-version state;
//! `ArchiveLeaseStatus` and `WorkDiagnostics` describe current or stranded work. Family failure
//! counts make incomplete acquisition visible.
//!
//! Diagnostics are observations of an already opened archive. They do not migrate, resume, or
//! clear work. This separation lets the CLI explain what a repair command would affect before the
//! user runs it.
//!
//! Schema inspection validates applied migration checksums before returning pending migrations.
//! A successful result therefore has valid history; invalid history is an error rather than a
//! diagnostic record with `history_valid = false`. Pending migrations are not applied here.
//!
//! Lease inspection compares the persisted expiry with the current process clock. `held` describes
//! that read moment and grants no write capability: mutations still require a current fencing token
//! inside their own transactions. Released or expired leases retain their diagnostic coordinates.
//!
//! Work counters and the three diagnostic sections use separate reads, so concurrent writers may
//! advance between them. Totals are useful operator observations rather than a frozen accounting
//! snapshot. Failures without a family are counted separately; the unresolved total includes every
//! unresolved ledger entry, even one with a family label this binary does not recognize.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::timestamp::UtcTimestamp;
use serde::Serialize;
use sqlx::Row;

use crate::archive::Archive;
use crate::clock::now_utc;
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
    ///
    /// Successful diagnostics always report true. Invalid history returns an error before this
    /// value is constructed; callers must handle that error rather than await a false flag.
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
    /// True only when an owner exists and expiry is in the future at inspection time.
    ///
    /// This is diagnostic state, not permission to write. Another owner may acquire the lease
    /// after the query; mutations must independently validate their archive fencing token.
    pub held: bool,
}

/// Observed job statuses and unresolved failure-ledger counts.
///
/// Jobs, runs, and failures are different units: one job can have multiple failure entries, and
/// resolving an entry does not imply its job status changed. These counters are not additive.
/// Family counts include known labels only; the total also includes unknown non-null labels.
/// Separate reads may observe different moments, so family subtotals need not reconcile during
/// concurrent writes. Use run details and retry policy to decide which work can be retried.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkDiagnostics {
    /// Jobs whose last recorded status is failed.
    pub failed_jobs: u64,
    /// Jobs whose last recorded status is deferred.
    pub deferred_jobs: u64,
    /// Runs currently recorded as in progress, whether active or left by an interrupted process.
    ///
    /// This count alone cannot distinguish a live worker from abandoned work; inspect the lease
    /// and run ledger before choosing recovery.
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
    /// Known evidence family assigned to unresolved ledger entries.
    ///
    /// Membership here does not establish that every entry is retryable under workflow policy.
    pub family: EvidenceFamily,
    /// Number of unresolved failures for this family.
    pub unresolved: u64,
}

impl Archive {
    /// Reads archive diagnostics without acquiring a write lease or changing archive state.
    ///
    /// Schema, lease, and work sections are read in that order. Their separate queries can observe
    /// different moments during concurrent writes. Inspect pending migrations to prepare an
    /// explicit migration; inspect lease/work state to explain recovery without performing it
    /// here.
    ///
    /// # Errors
    ///
    /// Invalid migration history stops inspection before lease/work queries. Invalid stored counts,
    /// timestamps, out-of-range process time, and SQL failures return typed archive errors. No
    /// failed or successful inspection repairs evidence, resolves a failure, or takes over a
    /// writer lease.
    pub async fn diagnostics(&self) -> Result<ArchiveDiagnostics, StoreError> {
        Ok(ArchiveDiagnostics {
            schema: self.schema_diagnostics().await?,
            lease: self.lease_diagnostics().await?,
            work: self.work_diagnostics().await?,
        })
    }

    /// Validates applied checksums before describing migrations available to this binary.
    async fn schema_diagnostics(&self) -> Result<SchemaDiagnostics, StoreError> {
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

        Ok(SchemaDiagnostics {
            current_version,
            supported_version,
            pending_migrations,
            history_valid: true,
        })
    }

    /// Compares the persisted lease expiry with the process clock without taking ownership.
    async fn lease_diagnostics(&self) -> Result<ArchiveLeaseStatus, StoreError> {
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
        let now = now_utc()?;
        let held = owner_id.is_some() && expires_at > now;
        let fencing_token =
            u64::try_from(fencing_token).map_err(|_| StoreError::InvalidStoredCount)?;

        Ok(ArchiveLeaseStatus {
            owner_id,
            fencing_token,
            expires_at,
            held,
        })
    }

    /// Reads work and failure counters independently; this is an observation, not a snapshot lock.
    async fn work_diagnostics(&self) -> Result<WorkDiagnostics, StoreError> {
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
        let failures_by_family = self.family_failure_counts().await?;
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

        Ok(WorkDiagnostics {
            failed_jobs,
            deferred_jobs,
            in_progress_runs,
            failures_by_family,
            unassigned_failures,
            unresolved_failures,
        })
    }

    /// Counts unresolved entries for each known family in the fixed presentation order.
    ///
    /// Unknown family labels are omitted here and remain part of the separate unresolved total.
    /// Each family query is independent; this projection does not acquire a snapshot or write
    /// lease.
    async fn family_failure_counts(&self) -> Result<Vec<FamilyFailureCount>, StoreError> {
        let mut counts = Vec::with_capacity(5);
        for (family, name) in [
            (EvidenceFamily::Threads, "threads"),
            (EvidenceFamily::Comments, "comments"),
            (EvidenceFamily::PullRequestMetadata, "pull_request_metadata"),
            (EvidenceFamily::Reviews, "reviews"),
            (EvidenceFamily::ReviewThreads, "review_threads"),
        ] {
            counts.push(FamilyFailureCount {
                family,
                unresolved: count_bound(
                    &self.reader,
                    "SELECT COUNT(*) FROM failures WHERE resolved_at_us IS NULL AND family = ?",
                    name,
                )
                .await?,
            });
        }
        Ok(counts)
    }
}

/// Counts one diagnostic category without changing archive state.
async fn count(pool: &sqlx::SqlitePool, sql: &'static str) -> Result<u64, StoreError> {
    let value: i64 = sqlx::query_scalar(sql).fetch_one(pool).await?;
    u64::try_from(value).map_err(|_| StoreError::InvalidStoredCount)
}

/// Counts a diagnostic category scoped by a bound archive value.
async fn count_bound(
    pool: &sqlx::SqlitePool,
    sql: &'static str,
    value: &str,
) -> Result<u64, StoreError> {
    let value: i64 = sqlx::query_scalar(sql).bind(value).fetch_one(pool).await?;
    u64::try_from(value).map_err(|_| StoreError::InvalidStoredCount)
}
