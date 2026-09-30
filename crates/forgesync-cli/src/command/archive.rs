//! # Create, inspect, migrate, and diagnose archives
//!
//! `ArchiveCommand` groups operations whose primary subject is the local database. Its run method
//! selects the explicit archive lifecycle action and renders the resulting status or diagnostic
//! report.
//!
//! Create, open, and migrate have different side effects. This command is where a user explicitly
//! asks for them; ordinary read commands must not silently create or change an archive.

use std::path::Path;
use std::process::ExitCode;

use clap::Subcommand;
use forgesync_engine::inspect::archive_status;
use forgesync_store::archive::Archive;

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
            Ok(status) => {
                let data = ArchiveStatusOutput::from(&status);
                render_success(
                    output,
                    "archive status",
                    &data,
                    ArchiveStatusOutput::summary,
                )
            }
            Err(error) => render_engine_error(output, "archive status", error),
        }
    }

    /// Checks integrity and returns failure when the archive is unhealthy.
    async fn doctor(path: &Path, output: OutputMode) -> ExitCode {
        let archive = match Archive::open_read_only(path).await {
            Ok(archive) => archive,
            Err(error) => return render_store_error(output, "archive doctor", error),
        };
        let result = archive.doctor().await;
        archive.close().await;
        match result {
            Ok(report) => {
                let exit_status = if report.healthy {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                };
                render_result(
                    output,
                    "archive doctor",
                    &report,
                    doctor_summary,
                    exit_status,
                )
            }
            Err(error) => render_store_error(output, "archive doctor", error),
        }
    }
}
