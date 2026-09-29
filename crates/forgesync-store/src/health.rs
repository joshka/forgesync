//! Archive integrity and health checks.
//!
//! Health checks inspect archive metadata and integrity locally. The report is suitable for a
//! doctor command; it does not repair or refresh provider data.

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

/// Local health report produced without changing persisted archive state.
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
    /// Checks archive integrity, foreign-key enforcement, and FTS5 availability.
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

async fn check_foreign_keys(pool: &sqlx::SqlitePool) -> HealthCheck {
    let result: Result<(), String> = async {
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
    .await;

    match result {
        Ok(()) => pass("foreign_keys", "foreign-key enforcement is enabled"),
        Err(detail) => fail("foreign_keys", detail),
    }
}

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

async fn check_fts5(pool: &sqlx::SqlitePool) -> HealthCheck {
    let result = async {
        let mut connection = pool.acquire().await.map_err(|_| ())?;
        sqlx::query("DROP TABLE IF EXISTS temp.__forgesync_fts5_probe")
            .execute(&mut *connection)
            .await
            .map_err(|_| ())?;
        let create_ok =
            sqlx::query("CREATE VIRTUAL TABLE temp.__forgesync_fts5_probe USING fts5(content)")
                .execute(&mut *connection)
                .await
                .is_ok();
        let probe_ok = if create_ok {
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM temp.__forgesync_fts5_probe")
                .fetch_one(&mut *connection)
                .await
                .is_ok()
        } else {
            false
        };
        let cleanup_ok = sqlx::query("DROP TABLE IF EXISTS temp.__forgesync_fts5_probe")
            .execute(&mut *connection)
            .await
            .is_ok();
        if !(create_ok && probe_ok && cleanup_ok) {
            return Err(());
        }
        Ok::<_, ()>(())
    }
    .await;

    match result {
        Ok(()) => pass("fts5", "SQLite FTS5 is available"),
        Err(()) => fail("fts5", "SQLite FTS5 is unavailable or could not be probed"),
    }
}

fn pass(name: &str, detail: impl Into<String>) -> HealthCheck {
    HealthCheck {
        name: name.to_owned(),
        healthy: true,
        detail: detail.into(),
    }
}

fn fail(name: &str, detail: impl Into<String>) -> HealthCheck {
    HealthCheck {
        name: name.to_owned(),
        healthy: false,
        detail: detail.into(),
    }
}
