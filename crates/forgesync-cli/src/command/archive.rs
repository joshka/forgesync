//! # Create, inspect, migrate, and diagnose archives
//!
//! [`ArchiveCommand`] groups operations whose primary subject is the local SQLite archive.
//! Its dispatch method selects an explicit lifecycle action; each operation owns opening and
//! closing its archive handle and then delegates presentation to the archive report module.
//! No variant acquires provider content or discovers GitHub credentials.
//!
//! Init creates a new archive without overwriting an existing file. Migrate applies pending schema
//! changes to an existing archive, then opens it read-only to report its resulting identity and
//! metadata. Migration and that reporting read are separate operations: an error during the later
//! read does not undo applied migrations. Store migration errors can also follow earlier committed
//! migrations, so failure must not be interpreted as an unchanged file.
//!
//! Status and doctor open the archive read-only. Status projects local metadata and counts through
//! the engine, then adapts that projection to the CLI output schema. Doctor asks the store to check
//! integrity and SQLite capabilities; its temporary capability probes do not change durable
//! archive data. Both require an archive accepted by the ordinary read-only opening checks.
//!
//! Doctor preserves a completed diagnostic report even when it is unhealthy, rendering that report
//! with a failing exit status. Failure to obtain a report uses the shared typed-error presentation.
//! A healthy report describes the selected checks, not provider freshness or completion of every
//! acquisition workflow. Creation, migration, and inspection remain distinct user choices.

use std::path::Path;
use std::process::ExitCode;

use clap::Subcommand;
use forgesync_engine::inspect::archive_status;
use forgesync_store::archive::Archive;
use forgesync_store::health::DoctorReport;
use forgesync_store::reads::ArchiveStatus;

use crate::output::ArchiveStatusOutput;
use crate::reports::archive::{
    MigrationOutput, archive_summary, doctor_summary, migration_summary,
};
use crate::{OutputMode, render_engine_error, render_result, render_store_error, render_success};

/// Explicit archive lifecycle operations.
#[derive(Clone, Debug, Subcommand)]
pub enum ArchiveCommand {
    /// Create a new archive without overwriting an existing file.
    Init,
    /// Apply pending schema migrations to an existing archive.
    Migrate,
    /// Show validated archive metadata without changing the archive.
    Status,
    /// Check archive integrity and required SQLite capabilities.
    Doctor,
}

impl ArchiveCommand {
    /// Runs the selected lifecycle operation and renders its process result.
    ///
    /// Creation and migration are explicit mutations. Status and doctor open the archive read-only.
    pub async fn run(self, path: &Path, output: OutputMode) -> ExitCode {
        match self {
            Self::Init => Self::init(path, output).await,
            Self::Migrate => Self::migrate(path, output).await,
            Self::Status => Self::status(path, output).await,
            Self::Doctor => Self::doctor(path, output).await,
        }
    }

    /// Creates a new archive and reports its stored identity.
    async fn init(path: &Path, output: OutputMode) -> ExitCode {
        let archive = match Archive::create(path).await {
            Ok(archive) => archive,
            Err(error) => return render_store_error(output, "archive init", error),
        };
        let info = archive.info().clone();
        archive.close().await;
        render_success(output, "archive init", &info, archive_summary)
    }

    /// Applies migrations, then reads the resulting archive metadata for the report.
    ///
    /// A failure opening the reporting handle leaves successful migrations in place. The store
    /// also permits earlier migrations to remain committed if a later migration fails.
    async fn migrate(path: &Path, output: OutputMode) -> ExitCode {
        let migration = match Archive::migrate(path).await {
            Ok(migration) => migration,
            Err(error) => return render_store_error(output, "archive migrate", error),
        };
        let archive = match Archive::open_read_only(path).await {
            Ok(archive) => archive,
            Err(error) => return render_store_error(output, "archive migrate", error),
        };
        let data = MigrationOutput {
            migration,
            archive: archive.info().clone(),
        };
        archive.close().await;
        render_success(output, "archive migrate", &data, migration_summary)
    }

    /// Reads archive status without changing schema or provider content.
    async fn status(path: &Path, output: OutputMode) -> ExitCode {
        let archive = match Archive::open_read_only(path).await {
            Ok(archive) => archive,
            Err(error) => return render_store_error(output, "archive status", error),
        };
        let result = archive_status(&archive).await;
        archive.close().await;
        match result {
            Ok(status) => Self::present_status(output, &status),
            Err(error) => render_engine_error(output, "archive status", error),
        }
    }

    /// Presents an already-read status using the CLI schema and corresponding text summary.
    ///
    /// Conversion borrows the store report; no archive handle remains open and no additional
    /// inspection is performed during presentation.
    fn present_status(output: OutputMode, status: &ArchiveStatus) -> ExitCode {
        let data = ArchiveStatusOutput::from(status);
        render_success(
            output,
            "archive status",
            &data,
            ArchiveStatusOutput::summary,
        )
    }

    /// Checks integrity and returns failure when the archive is unhealthy.
    ///
    /// An unhealthy completed report remains report data in both output modes. A failure to open
    /// the archive or perform a check is rendered as a store error instead.
    async fn doctor(path: &Path, output: OutputMode) -> ExitCode {
        let archive = match Archive::open_read_only(path).await {
            Ok(archive) => archive,
            Err(error) => return render_store_error(output, "archive doctor", error),
        };
        let result = archive.doctor().await;
        archive.close().await;
        match result {
            Ok(report) => Self::present_doctor(output, &report),
            Err(error) => render_store_error(output, "archive doctor", error),
        }
    }

    /// Presents completed health checks as report data, preserving unhealthy failure status.
    ///
    /// An unhealthy report remains a successful report envelope rather than a command error;
    /// its process status communicates health in both output modes. Rendering can itself fail
    /// through the shared output layer.
    fn present_doctor(output: OutputMode, report: &DoctorReport) -> ExitCode {
        let exit_status = if report.healthy {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
        render_result(
            output,
            "archive doctor",
            report,
            doctor_summary,
            exit_status,
        )
    }
}
