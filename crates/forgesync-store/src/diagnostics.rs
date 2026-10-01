//! Read-only schema, lease, and pending-work diagnostics.
//!
//! Sections come from separate reads, so concurrent writers can advance between them.

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
use crate::sql::{ALL_FAMILIES, count_from_sql, parse_family, timestamp_from_sql};

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
    /// Always true: invalid history is returned as an error instead. Kept for the JSON contract.
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
    pub held: bool,
}

/// Observed job statuses and unresolved failure-ledger counts.
///
/// Jobs, runs, and failures are different units, so these counters are not additive. Family
/// counts include known labels only; the unresolved total also includes unknown labels.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkDiagnostics {
    /// Jobs whose last recorded status is failed.
    pub failed_jobs: u64,
    /// Jobs whose last recorded status is deferred.
    pub deferred_jobs: u64,
    /// Runs recorded as in progress, whether active or left by an interrupted process.
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
    pub family: EvidenceFamily,
    /// Number of unresolved failures for this family.
    pub unresolved: u64,
}

impl Archive {
    /// Reads archive diagnostics without acquiring a write lease or changing archive state.
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
        let expires_at = timestamp_from_sql(expires_at_us)?;
        let now = now_utc()?;
        let held = owner_id.is_some() && expires_at > now;
        let fencing_token = count_from_sql(fencing_token)?;

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
    async fn family_failure_counts(&self) -> Result<Vec<FamilyFailureCount>, StoreError> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT family, COUNT(*) FROM failures WHERE resolved_at_us IS NULL AND family IS NOT NULL GROUP BY family",
        )
        .fetch_all(&self.reader)
        .await?;
        let mut counts = ALL_FAMILIES
            .into_iter()
            .map(|family| FamilyFailureCount {
                family,
                unresolved: 0,
            })
            .collect::<Vec<_>>();
        for (name, count) in rows {
            if let Some(family) = parse_family(&name)
                && let Some(entry) = counts.iter_mut().find(|entry| entry.family == family)
            {
                entry.unresolved = count_from_sql(count)?;
            }
        }
        Ok(counts)
    }
}

/// Counts one diagnostic category without changing archive state.
async fn count(pool: &sqlx::SqlitePool, sql: &'static str) -> Result<u64, StoreError> {
    count_from_sql(sqlx::query_scalar(sql).fetch_one(pool).await?)
}
