//! Archive health checks; the doctor reports problems rather than repairing them.

use serde::Serialize;

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

/// Integrity and capability report.
///
/// `healthy` summarizes `checks` only; pending work or an active lease can still appear in
/// `diagnostics` on a healthy report.
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
    /// Check failures become unhealthy entries; only diagnostics failures return an error.
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

/// Checks that the reader connection enforces foreign keys.
async fn check_foreign_keys(pool: &sqlx::SqlitePool) -> HealthCheck {
    match sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys")
        .fetch_one(pool)
        .await
    {
        Ok(1) => pass("foreign_keys", "foreign-key enforcement is enabled"),
        Ok(_) => fail("foreign_keys", "foreign_keys pragma is disabled"),
        Err(_) => fail("foreign_keys", "could not read the foreign_keys pragma"),
    }
}

/// Checks the full-text capability required by offline keyword search by reading the archive's
/// FTS5 table, which fails when the module is unavailable.
async fn check_fts5(pool: &sqlx::SqlitePool) -> HealthCheck {
    match sqlx::query("SELECT rowid FROM thread_search LIMIT 0")
        .fetch_optional(pool)
        .await
    {
        Ok(_) => pass("fts5", "SQLite FTS5 is available"),
        Err(_) => fail("fts5", "SQLite FTS5 is unavailable or could not be probed"),
    }
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
