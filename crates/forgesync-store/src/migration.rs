//! # Ordered schema changes and migration reporting
//!
//! `AppliedMigration` and `MigrationReport` describe version changes made by an explicit migration
//! operation. Migration files are ordered, immutable history of the on-disk schema; this module
//! checks and applies them.
//!
//! Opening an archive does not migrate it. Keeping the migration path separate lets status and
//! diagnostics inspect supported history without unexpectedly applying SQL. Ordinary opening can
//! reject a schema that requires migration; the explicit lifecycle path handles that operation.
//!
//! `MIGRATOR` embeds the immutable SQL catalog in the binary. `current_schema_version` requires
//! successful recorded history and rejects a dirty record; `validate_migration_history` separately
//! compares successful records with embedded versions and checksums. Callers use both checks in
//! that order. Neither helper silently repairs a missing table, unknown version, or altered SQL.
//!
//! `apply_pending_migrations` is an implementation operation for explicit lifecycle callers, not
//! a general public pool API. SQLx applies pending migrations using its migration machinery, then
//! this module observes the resulting version and constructs [`MigrationReport`]. The supplied
//! prior version is the caller's validated baseline; the report lists embedded versions between
//! that baseline and the observed final version.
//!
//! A failed multi-migration operation need not roll back earlier successfully applied migrations.
//! No report is returned on failure; the next attempt must inspect durable migration history
//! rather than assuming the original baseline is still current. Public report types expose schema
//! facts, while pool-level helpers retain crate visibility to preserve the archive lifecycle seam.

use serde::Serialize;
use sqlx::{Row, SqlitePool};

use crate::error::StoreError;

/// Embedded immutable schema catalog shared by lifecycle and diagnostic implementation.
pub(crate) static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

/// A migration applied by an explicit archive migration operation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AppliedMigration {
    /// Ordered migration version.
    pub version: i64,
    /// Short description from the embedded migration file.
    pub description: String,
}

/// Schema transition reported after an explicit migration operation succeeds.
///
/// Entries follow embedded migration order and describe versions newer than the validated baseline
/// and no newer than the observed final version. This is an operation result, not a recovery log;
/// failed operations return an error and can have applied earlier migrations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MigrationReport {
    /// Schema version before migration.
    pub previous_schema_version: i64,
    /// Schema version after migration.
    pub schema_version: i64,
    /// Migrations applied during this operation.
    pub applied_migrations: Vec<AppliedMigration>,
}

/// Reports the newest embedded migration understood by this binary.
pub(crate) fn supported_schema_version() -> i64 {
    MIGRATOR
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap_or(0)
}

/// Reads the highest successful version after requiring present, nondirty migration history.
///
/// Does not compare checksums or reject unknown successful versions; the caller next invokes
/// `validate_migration_history`. Missing/empty history and dirty records have distinct errors.
/// This query applies no SQL migrations and propagates database failures.
pub(crate) async fn current_schema_version(pool: &SqlitePool) -> Result<i64, StoreError> {
    let has_migration_table: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations')",
    )
    .fetch_one(pool)
    .await?;

    if has_migration_table == 0 {
        return Err(StoreError::MigrationHistoryMissing);
    }

    let dirty_version: Option<i64> =
        sqlx::query_scalar("SELECT MIN(version) FROM _sqlx_migrations WHERE success = 0")
            .fetch_one(pool)
            .await?;
    if let Some(version) = dirty_version {
        return Err(StoreError::MigrationHistoryDirty { version });
    }

    let version: Option<i64> =
        sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1")
            .fetch_one(pool)
            .await?;

    version.ok_or(StoreError::MigrationHistoryMissing)
}

/// Compares successful recorded versions and checksums with the embedded catalog.
///
/// Requires a baseline obtained from `current_schema_version`, which owns dirty-history detection.
/// This helper reads successful rows only; it does not independently reject dirty records or prove
/// that the supplied baseline matches the database. Unknown, newer, or checksum-mismatched versions
/// return typed errors without rewriting history. Database failures are propagated.
pub(crate) async fn validate_migration_history(
    pool: &SqlitePool,
    schema_version: i64,
) -> Result<(), StoreError> {
    let rows = sqlx::query(
        "SELECT version, checksum FROM _sqlx_migrations WHERE success = 1 ORDER BY version",
    )
    .fetch_all(pool)
    .await?;

    for row in rows {
        let version: i64 = row.try_get("version")?;
        if version > supported_schema_version() {
            return Err(StoreError::SchemaTooNew {
                found: version,
                supported: supported_schema_version(),
            });
        }

        let migration = MIGRATOR
            .iter()
            .find(|migration| migration.version == version)
            .ok_or(StoreError::MigrationVersionUnknown { version })?;
        let checksum: Vec<u8> = row.try_get("checksum")?;
        if migration.checksum.as_ref() != checksum.as_slice() {
            return Err(StoreError::MigrationChecksumMismatch { version });
        }
    }

    if schema_version > supported_schema_version() {
        return Err(StoreError::SchemaTooNew {
            found: schema_version,
            supported: supported_schema_version(),
        });
    }
    Ok(())
}

/// Applies pending embedded SQL and reports versions after the caller's validated baseline.
///
/// Lifecycle callers validate archive metadata and history before passing the pool and prior
/// version. A newer baseline is rejected before application. SQLx owns migration execution; after
/// success this helper rereads the version and reports embedded entries in the resulting interval.
///
/// Migration or follow-up read failure returns no report. Earlier successful migrations can remain
/// durable, so recovery must revalidate the database rather than blindly reuse the prior baseline.
pub(crate) async fn apply_pending_migrations(
    pool: &SqlitePool,
    previous_schema_version: i64,
) -> Result<MigrationReport, StoreError> {
    let supported = supported_schema_version();
    if previous_schema_version > supported {
        return Err(StoreError::SchemaTooNew {
            found: previous_schema_version,
            supported,
        });
    }

    MIGRATOR.run(pool).await?;
    let schema_version = current_schema_version(pool).await?;
    let applied_migrations = MIGRATOR
        .iter()
        .filter(|migration| {
            migration.version > previous_schema_version && migration.version <= schema_version
        })
        .map(|migration| AppliedMigration {
            version: migration.version,
            description: migration.description.to_string(),
        })
        .collect();

    Ok(MigrationReport {
        previous_schema_version,
        schema_version,
        applied_migrations,
    })
}
