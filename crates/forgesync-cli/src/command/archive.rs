//! Create, migrate, inspect, and diagnose archives.
//!
//! Migration and the read that reports its result are separate operations: a failure in the later
//! read, or in a later migration step, does not undo migrations that already committed.

use std::path::Path;

use clap::Subcommand;
use forgesync_engine::inspect::archive_status;
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;

use super::with_archive;
use crate::error::{CliError, Exit};
use crate::output::Output;
use crate::reports::archive::{
    MigrationOutput, archive_status_summary, archive_summary, doctor_summary, migration_summary,
};

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
    pub async fn run(self, path: &Path, output: Output) -> Result<Exit, CliError> {
        match self {
            Self::Init => {
                create_parent_directory(path)?;
                let archive = Archive::create(path).await?;
                let info = archive.info().clone();
                archive.close().await;
                Ok(output.success(&info, archive_summary))
            }
            Self::Migrate => {
                let migration = Archive::migrate(path).await?;
                let archive = Archive::open_read_only(path).await?;
                let data = MigrationOutput {
                    migration,
                    archive: archive.info().clone(),
                };
                archive.close().await;
                Ok(output.success(&data, migration_summary))
            }
            Self::Status => {
                let status = with_archive(Archive::open_read_only(path), archive_status).await?;
                Ok(output.success(&status, archive_status_summary))
            }
            Self::Doctor => {
                let report = with_archive(Archive::open_read_only(path), async |archive| {
                    archive.doctor().await
                })
                .await?;
                // An unhealthy report is still report data, with a failing status.
                let exit = if report.healthy {
                    Exit::Success
                } else {
                    Exit::Failure
                };
                Ok(output.report(&report, doctor_summary, exit))
            }
        }
    }
}

/// Creates the database's parent directory; only explicit initialization does this.
fn create_parent_directory(path: &Path) -> Result<(), StoreError> {
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(());
    };
    std::fs::create_dir_all(parent).map_err(|source| StoreError::Io {
        path: parent.to_path_buf(),
        source,
    })
}
