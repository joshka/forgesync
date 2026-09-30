//! # Health checks and repair guidance
//!
//! `HealthCheck` describes an individual condition and `DoctorReport` collects the archive's
//! findings. `Archive` methods inspect schema, persisted work, and other local state that might
//! need attention.
//!
//! The doctor path reports problems rather than silently repairing them. Commands can present the
//! recommended next action while creation, migration, retry, and refresh remain explicit
//! operations with their own side effects.
//!
//! [`Archive::doctor`] first obtains [`ArchiveDiagnostics`], which validates schema history and
//! observes lease and pending-work state. It then reports four checks in a stable order: SQLite
//! integrity, foreign-key enforcement, FTS5 capability, and schema history. A failed capability
//! probe becomes an unhealthy [`HealthCheck`]; failure to acquire the diagnostic report returns
//! an error before a [`DoctorReport`] can be assembled.
//!
//! Probes use the reader pool and create connection-local temporary tables to exercise actual
//! SQLite behavior. They attempt cleanup before returning and never create permanent archive
//! tables. Thus this operation leaves durable content unchanged, but it is not SQL with no side
//! effects at all. Separate reads and pooled connections do not form one database snapshot.
//!
//! A healthy report means these checks passed, not that all provider evidence is complete or
//! pending work is absent. Inspect `diagnostics.work` and family coverage when assessing freshness;
//! the aggregate health flag is not permission to skip a later write's lease or identity checks.

use serde::Serialize;
use sqlx::SqliteConnection;

use crate::archive::Archive;
use crate::diagnostics::ArchiveDiagnostics;
use crate::error::StoreError;

/// Result of one local archive health check.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HealthCheck {
    /// Stable check name.
    pub name: String,
    /// Whether the check passed.
    pub healthy: bool,
    /// Short detail safe to show in terminal or JSON output.
    pub detail: String,
}

/// Local capability and integrity report that leaves durable archive state unchanged.
///
/// `healthy` summarizes `checks` only. Pending work, an active lease, or unapplied migrations can
/// still appear in `diagnostics` on a healthy report. The fields are observations from separate
/// queries rather than one transactionally consistent snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DoctorReport {
    /// True when all checks pass.
    pub healthy: bool,
    /// Individual health checks.
    pub checks: Vec<HealthCheck>,
    /// Read-only schema, lease, and retryable-work diagnostics.
    pub diagnostics: ArchiveDiagnostics,
}

impl Archive {
    /// Inspects integrity, foreign-key enforcement, FTS5 capability, and schema history.
    ///
    /// Runs against an already opened archive, including a read-only archive. SQLite probes use
    /// temporary tables on acquired reader connections and attempt to remove them before returning;
    /// no permanent content, checkpoint, lease, or migration is changed.
    ///
    /// Individual probe failures are retained as unhealthy checks so the caller can show the
    /// remaining results. Schema diagnostics are obtained first: their failure rejects the whole
    /// operation instead of producing a partial report. The checks and diagnostics are not one
    /// snapshot, and `healthy` does not summarize pending work or acquisition completeness.
    ///
    /// # Errors
    ///
    /// Returns errors from [`Archive::diagnostics`], including invalid migration history and
    /// diagnostic database failures. Capability-probe errors become report entries rather than
    /// errors from this method.
    pub async fn doctor(&self) -> Result<DoctorReport, StoreError> {
        let diagnostics = self.diagnostics().await?;
        let mut checks = Vec::with_capacity(4);
        checks.push(check_integrity(&self.reader).await);
        checks.push(check_foreign_keys(&self.reader).await);
        checks.push(check_fts5(&self.reader).await);
        checks.push(pass(
            "schema_history",
            format!(
                "migration history is valid at schema {} (binary supports {})",
                diagnostics.schema.current_version, diagnostics.schema.supported_version
            ),
        ));
        let healthy = checks.iter().all(|check| check.healthy);
        Ok(DoctorReport {
            healthy,
            checks,
            diagnostics,
        })
    }
}

/// Runs SQLite integrity checking and retains its diagnostic detail.
async fn check_integrity(pool: &sqlx::SqlitePool) -> HealthCheck {
    match sqlx::query_scalar::<_, String>("PRAGMA quick_check")
        .fetch_one(pool)
        .await
    {
        Ok(result) if result == "ok" => pass("integrity", "SQLite quick_check passed"),
        Ok(result) => fail("integrity", format!("SQLite quick_check reported {result}")),
        Err(_) => fail("integrity", "SQLite quick_check could not complete"),
    }
}

/// Checks whether required foreign-key enforcement works on this connection.
async fn check_foreign_keys(pool: &sqlx::SqlitePool) -> HealthCheck {
    let result = probe_foreign_keys(pool).await;

    match result {
        Ok(()) => pass("foreign_keys", "foreign-key enforcement is enabled"),
        Err(detail) => fail("foreign_keys", detail),
    }
}

/// Acquires one reader connection and verifies its setting and actual constraint enforcement.
///
/// The returned detail distinguishes acquisition, pragma, and violation-probe failures. Temporary
/// table creation and cleanup stay on that same connection; no durable archive rows are changed.
async fn probe_foreign_keys(pool: &sqlx::SqlitePool) -> Result<(), String> {
    let mut connection = pool
        .acquire()
        .await
        .map_err(|_| "could not acquire a SQLite connection".to_owned())?;
    let enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| "could not read the foreign_keys pragma".to_owned())?;
    if enabled != 1 {
        return Err("foreign_keys pragma is disabled".to_owned());
    }

    foreign_key_violation_is_enforced(&mut connection)
        .await
        .map_err(|_| "foreign-key enforcement probe failed".to_owned())?;
    Ok(())
}

/// Probes a temporary violation to verify SQLite rejects broken references.
async fn foreign_key_violation_is_enforced(connection: &mut SqliteConnection) -> Result<(), ()> {
    drop_foreign_key_probe_tables(connection).await?;
    let probe_result = async {
        sqlx::query("CREATE TEMP TABLE __forgesync_fk_parent (id INTEGER PRIMARY KEY)")
            .execute(&mut *connection)
            .await
            .map_err(|_| ())?;
        sqlx::query(
            "CREATE TEMP TABLE __forgesync_fk_child (parent_id INTEGER REFERENCES __forgesync_fk_parent(id))",
        )
        .execute(&mut *connection)
        .await
        .map_err(|_| ())?;
        let insert_result =
            sqlx::query("INSERT INTO __forgesync_fk_child (parent_id) VALUES (1)")
                .execute(&mut *connection)
                .await;
        match insert_result {
            Err(sqlx::Error::Database(error))
                if error.message().contains("FOREIGN KEY constraint failed") =>
            {
                Ok(())
            }
            _ => Err(()),
        }
    }
    .await;
    let cleanup_result = drop_foreign_key_probe_tables(connection).await;

    probe_result.and(cleanup_result)
}

/// Removes temporary integrity-probe tables after the check.
async fn drop_foreign_key_probe_tables(connection: &mut SqliteConnection) -> Result<(), ()> {
    sqlx::query("DROP TABLE IF EXISTS temp.__forgesync_fk_child")
        .execute(&mut *connection)
        .await
        .map_err(|_| ())?;
    sqlx::query("DROP TABLE IF EXISTS temp.__forgesync_fk_parent")
        .execute(&mut *connection)
        .await
        .map_err(|_| ())?;
    Ok(())
}

/// Checks the full-text capability required by offline keyword search.
async fn check_fts5(pool: &sqlx::SqlitePool) -> HealthCheck {
    let result = probe_fts5(pool).await;

    match result {
        Ok(()) => pass("fts5", "SQLite FTS5 is available"),
        Err(()) => fail("fts5", "SQLite FTS5 is unavailable or could not be probed"),
    }
}

/// Exercises FTS5 on one reader connection and attempts cleanup after creation or query failure.
///
/// Both the capability operation and final cleanup must succeed. An initial cleanup failure
/// returns immediately, since the connection cannot safely reuse the probe table name.
async fn probe_fts5(pool: &sqlx::SqlitePool) -> Result<(), ()> {
    let mut connection = pool.acquire().await.map_err(|_| ())?;
    drop_fts5_probe_table(&mut connection).await?;
    let probe_result = create_and_query_fts5_table(&mut connection).await;
    let cleanup_result = drop_fts5_probe_table(&mut connection).await;
    probe_result.and(cleanup_result)
}

/// Creates a temporary FTS5 table and reads it to prove the extension can execute a query.
///
/// Creation failure skips the query. The caller owns cleanup even when this operation fails.
async fn create_and_query_fts5_table(connection: &mut SqliteConnection) -> Result<(), ()> {
    sqlx::query("CREATE VIRTUAL TABLE temp.__forgesync_fts5_probe USING fts5(content)")
        .execute(&mut *connection)
        .await
        .map_err(|_| ())?;
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM temp.__forgesync_fts5_probe")
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| ())?;
    Ok(())
}

/// Removes the connection-local FTS probe table, including remnants of an earlier failed probe.
async fn drop_fts5_probe_table(connection: &mut SqliteConnection) -> Result<(), ()> {
    sqlx::query("DROP TABLE IF EXISTS temp.__forgesync_fts5_probe")
        .execute(&mut *connection)
        .await
        .map_err(|_| ())?;
    Ok(())
}

/// Builds a successful named archive health check.
fn pass(name: &str, detail: impl Into<String>) -> HealthCheck {
    HealthCheck {
        name: name.to_owned(),
        healthy: true,
        detail: detail.into(),
    }
}

/// Builds a failed named archive health check with its diagnostic reason.
fn fail(name: &str, detail: impl Into<String>) -> HealthCheck {
    HealthCheck {
        name: name.to_owned(),
        healthy: false,
        detail: detail.into(),
    }
}
