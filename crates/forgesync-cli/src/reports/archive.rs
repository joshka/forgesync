//! Archive command presentation.

use forgesync_store::archive::ArchiveInfo;
use forgesync_store::health::DoctorReport;

use crate::output::ArchiveStatusOutput;
use crate::reports::{MigrationOutput, family_name};

pub fn archive_status_summary(status: &ArchiveStatusOutput<'_>) -> String {
    let mut lines = vec![
        archive_summary(status.archive),
        format!(
            "Repositories: {}\nThreads: {} ({} issues, {} pull requests)",
            status.repositories, status.threads, status.issues, status.pull_requests
        ),
        "Coverage:".to_owned(),
    ];
    lines.extend(status.coverage.iter().map(|coverage| {
        format!(
            "  {}: {} complete, {} incomplete, {} missing of {}",
            family_name(coverage.family),
            coverage.complete,
            coverage.incomplete,
            coverage.missing,
            coverage.applicable_threads
        )
    }));
    lines.push(format!(
        "Work: {} unresolved failures, {} failed jobs, {} deferred jobs, {} in-progress runs",
        status.diagnostics.work.unresolved_failures,
        status.diagnostics.work.failed_jobs,
        status.diagnostics.work.deferred_jobs,
        status.diagnostics.work.in_progress_runs
    ));
    lines.push(format!(
        "Lease: {} (fence {}, expires {})",
        if status.diagnostics.lease.held {
            format!(
                "held by {}",
                status
                    .diagnostics
                    .lease
                    .owner_id
                    .as_deref()
                    .unwrap_or("unknown")
            )
        } else {
            "available".to_owned()
        },
        status.diagnostics.lease.fencing_token,
        status
            .diagnostics
            .lease
            .expires_at
            .format_rfc3339()
            .unwrap_or_else(|_| "invalid timestamp".to_owned())
    ));
    lines.push(format!(
        "Schema: {} / {} supported{}",
        status.diagnostics.schema.current_version,
        status.diagnostics.schema.supported_version,
        if status.diagnostics.schema.history_valid {
            " (history valid)"
        } else {
            " (history invalid)"
        }
    ));
    lines.join("\n")
}

pub fn archive_summary(info: &ArchiveInfo) -> String {
    format!(
        "Archive: {}\nID: {}\nFormat: {}\nSchema: {}\nCreated: {}\nSQLite: {}",
        info.path,
        info.archive_id,
        info.format_id,
        info.schema_version,
        info.created_at
            .format_rfc3339()
            .unwrap_or_else(|_| "invalid timestamp".to_owned()),
        info.sqlite_version
    )
}

pub fn migration_summary(output: &MigrationOutput) -> String {
    let applied = output.migration.applied_migrations.len();
    format!(
        "Archive schema is at version {} ({} migration{} applied)",
        output.migration.schema_version,
        applied,
        if applied == 1 { "" } else { "s" }
    )
}

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
