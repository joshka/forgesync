//! # Explain archive state and maintenance results
//!
//! Archive summary functions turn creation, status, migration, and doctor results into readable
//! terminal output. They expose what changed or needs attention without taking the maintenance
//! action themselves.
//!
//! The archive lifecycle remains explicit in `command/archive`; this module only presents the
//! typed result. Keep wording aligned with actual schema and health reports rather than inferring
//! success from an opened handle.
//!
//! [`MigrationOutput`] pairs explicit migration steps with the resulting archive metadata. Status
//! and doctor summaries consume store projections; they never infer an upgrade from a successful
//! open. Commands close archive resources before presenting these values.

use forgesync_store::archive::ArchiveInfo;
use forgesync_store::health::DoctorReport;
use forgesync_store::migration::MigrationReport;
use serde::Serialize;

use crate::output::ArchiveStatusOutput;
use crate::reports::threads::family_name;

/// Migration outcome paired with archive identity and counts after the explicit upgrade.
///
/// This CLI DTO keeps migration details and the resulting archive together for JSON and human
/// rendering. It is built after migration completes and does not itself open or alter the archive.
#[derive(Debug, Serialize)]
pub struct MigrationOutput {
    /// Ordered migration steps and schema versions reported by the store.
    pub migration: MigrationReport,
    /// Archive metadata observed after those steps completed.
    pub archive: ArchiveInfo,
}

impl ArchiveStatusOutput<'_> {
    /// Presents the observed archive identity, coverage, work, lease, and schema in that order.
    ///
    /// These diagnostics describe the completed local read. A displayed lease is not authority to
    /// write, and rendering does not refresh or otherwise change the archive.
    pub fn summary(&self) -> String {
        let mut lines = vec![archive_summary(self.archive), self.counts_line()];
        lines.extend(self.coverage_lines());
        lines.extend([self.work_line(), self.lease_line(), self.schema_line()]);
        lines.join("\n")
    }

    /// Shows repository and discussion totals with the issue/pull-request split.
    fn counts_line(&self) -> String {
        format!(
            "Repositories: {}\nThreads: {} ({} issues, {} pull requests)",
            self.repositories, self.threads, self.issues, self.pull_requests
        )
    }

    /// Keeps coverage families in the store projection's order, including an empty heading.
    fn coverage_lines(&self) -> Vec<String> {
        let mut lines = vec!["Coverage:".to_owned()];
        lines.extend(self.coverage.iter().map(|coverage| {
            format!(
                "  {}: {} complete, {} incomplete, {} missing of {}",
                family_name(coverage.family),
                coverage.complete,
                coverage.incomplete,
                coverage.missing,
                coverage.applicable_threads
            )
        }));
        lines
    }

    /// Reports outstanding work without treating deferred jobs as completed work.
    fn work_line(&self) -> String {
        let work = &self.diagnostics.work;
        format!(
            "Work: {} unresolved failures, {} failed jobs, {} deferred jobs, {} in-progress runs",
            work.unresolved_failures, work.failed_jobs, work.deferred_jobs, work.in_progress_runs
        )
    }

    /// Describes the observed lease, retaining explicit fallbacks for missing owner or timestamp.
    fn lease_line(&self) -> String {
        let lease = &self.diagnostics.lease;
        let state = if lease.held {
            let owner = lease.owner_id.as_deref().unwrap_or("unknown");
            format!("held by {owner}")
        } else {
            "available".to_owned()
        };
        let expires = lease
            .expires_at
            .format_rfc3339()
            .unwrap_or_else(|_| "invalid timestamp".to_owned());
        format!(
            "Lease: {state} (fence {}, expires {expires})",
            lease.fencing_token
        )
    }

    /// Distinguishes the stored version, supported version, and migration-history diagnosis.
    fn schema_line(&self) -> String {
        let schema = &self.diagnostics.schema;
        let history = if schema.history_valid {
            " (history valid)"
        } else {
            " (history invalid)"
        };
        format!(
            "Schema: {} / {} supported{history}",
            schema.current_version, schema.supported_version
        )
    }
}

/// Formats stable metadata after creating or opening an archive.
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
