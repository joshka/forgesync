//! Archive lifecycle, status, migration, and health summaries.

use forgesync_store::archive::ArchiveInfo;
use forgesync_store::health::DoctorReport;
use forgesync_store::migration::MigrationReport;
use forgesync_store::reads::ArchiveStatus;
use serde::Serialize;

use crate::reports::pages::coverage_line;
use crate::reports::threads::format_timestamp;

/// Migration steps paired with the archive metadata observed after them.
#[derive(Debug, Serialize)]
pub struct MigrationOutput {
    pub migration: MigrationReport,
    pub archive: ArchiveInfo,
}

/// Presents archive identity, counts, coverage, work, lease, and schema in that order.
///
/// A displayed lease is an observation, not authority to write.
pub fn archive_status_summary(status: &ArchiveStatus) -> String {
    let mut lines = vec![
        archive_summary(&status.archive),
        format!(
            "Repositories: {}\nThreads: {} ({} issues, {} pull requests)",
            status.repositories, status.threads, status.issues, status.pull_requests
        ),
        "Coverage:".to_owned(),
    ];
    lines.extend(status.coverage.iter().map(coverage_line));

    let work = &status.diagnostics.work;
    lines.push(format!(
        "Work: {} unresolved failures, {} failed jobs, {} deferred jobs, {} in-progress runs",
        work.unresolved_failures, work.failed_jobs, work.deferred_jobs, work.in_progress_runs
    ));

    let lease = &status.diagnostics.lease;
    let state = if lease.held {
        format!("held by {}", lease.owner_id.as_deref().unwrap_or("unknown"))
    } else {
        "available".to_owned()
    };
    lines.push(format!(
        "Lease: {state} (fence {}, expires {})",
        lease.fencing_token,
        format_timestamp(lease.expires_at)
    ));

    let schema = &status.diagnostics.schema;
    let history = if schema.history_valid {
        "valid"
    } else {
        "invalid"
    };
    lines.push(format!(
        "Schema: {} / {} supported (history {history})",
        schema.current_version, schema.supported_version
    ));
    lines.join("\n")
}

/// Formats stable metadata after creating or opening an archive.
pub fn archive_summary(info: &ArchiveInfo) -> String {
    format!(
        "Archive: {}\nID: {}\nFormat: {}\nSchema: {}\nCreated: {}\nSQLite: {}",
        info.path,
        info.archive_id,
        info.format_id,
        info.schema_version,
        format_timestamp(info.created_at),
        info.sqlite_version
    )
}

/// Explains applied migrations and the resulting archive version.
pub fn migration_summary(output: &MigrationOutput) -> String {
    let applied = output.migration.applied_migrations.len();
    format!(
        "Archive schema is at version {} ({} migration{} applied)",
        output.migration.schema_version,
        applied,
        if applied == 1 { "" } else { "s" }
    )
}

/// Formats integrity checks while preserving each failed capability.
pub fn doctor_summary(report: &DoctorReport) -> String {
    let headline = if report.healthy {
        "Archive health: healthy"
    } else {
        "Archive health: unhealthy"
    };
    let checks = report
        .checks
        .iter()
        .map(|check| {
            let state = if check.healthy { "ok" } else { "failed" };
            format!("[{state}] {}: {}", check.name, check.detail)
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{headline}\n{checks}\n{} unresolved failures, {} failed jobs, {} deferred jobs\nSchema {} / {}, lease {}",
        report.diagnostics.work.unresolved_failures,
        report.diagnostics.work.failed_jobs,
        report.diagnostics.work.deferred_jobs,
        report.diagnostics.schema.current_version,
        report.diagnostics.schema.supported_version,
        if report.diagnostics.lease.held {
            "held"
        } else {
            "available"
        }
    )
}
