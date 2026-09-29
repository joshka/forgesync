//! Ordered schema migration reports.
//!
//! Migrations are ordered and immutable. Inspect pending changes separately from applying them so
//! callers can present an explicit archive upgrade.

use serde::Serialize;
use sqlx::{Row, SqlitePool};

use crate::error::StoreError;

pub(crate) static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

/// A migration applied by an explicit archive migration operation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AppliedMigration {
    /// Ordered migration version.
    pub version: i64,
    /// Short description from the embedded migration file.
    pub description: String,
}

/// Summary of an explicit migration operation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MigrationReport {
    /// Schema version before migration.
    pub previous_schema_version: i64,
    /// Schema version after migration.
    pub schema_version: i64,
    /// Migrations applied during this operation.
    pub applied_migrations: Vec<AppliedMigration>,
}

pub(crate) fn supported_schema_version() -> i64 {
    MIGRATOR
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap_or(0)
}

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
