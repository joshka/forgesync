use std::path::PathBuf;

use thiserror::Error;

/// Errors returned by archive lifecycle and local storage operations.
#[derive(Debug, Error)]
pub enum StoreError {
    /// An archive already occupies the requested path.
    #[error("archive already exists: {0}")]
    AlreadyExists(PathBuf),
    /// The requested archive path does not exist.
    #[error("archive does not exist: {0}")]
    MissingArchive(PathBuf),
    /// The requested archive path is not a regular file.
    #[error("archive path is not a regular file: {0}")]
    NotAFile(PathBuf),
    /// The database does not contain Forgesync archive metadata.
    #[error("archive metadata is missing")]
    MetadataMissing,
    /// The archive was created by an unsupported format.
    #[error("unsupported archive format: {0}")]
    UnsupportedFormat(String),
    /// The archive does not contain a successful migration history.
    #[error("archive migration history is missing")]
    MigrationHistoryMissing,
    /// A migration did not finish successfully.
    #[error("archive migration {version} is marked as incomplete")]
    MigrationHistoryDirty { version: i64 },
    /// The archive records a migration that this binary does not contain.
    #[error("archive migration {version} is not present in this binary")]
    MigrationVersionUnknown { version: i64 },
    /// The stored checksum for an applied migration differs from the embedded migration.
    #[error("archive migration {version} does not match the embedded migration")]
    MigrationChecksumMismatch { version: i64 },
    /// The archive must be explicitly migrated before it can be opened.
    #[error("archive schema version {current} requires migration to version {supported}")]
    MigrationRequired { current: i64, supported: i64 },
    /// The archive schema is newer than this binary supports.
    #[error("archive schema version {found} is newer than supported version {supported}")]
    SchemaTooNew { found: i64, supported: i64 },
    /// The stored archive UUID is malformed.
    #[error("archive ID is invalid")]
    InvalidArchiveId(#[source] uuid::Error),
    /// The stored creation timestamp cannot be represented by core timestamp rules.
    #[error("archive creation timestamp is invalid")]
    InvalidCreatedAt(#[source] forgesync_core::TimestampError),
    /// The system clock could not produce a supported archive timestamp.
    #[error("system clock is before the Unix epoch or outside the supported range")]
    ClockOutOfRange,
    /// An archive filesystem operation failed.
    #[error("archive filesystem operation failed for {path}: {source}")]
    Io {
        /// Path involved in the operation.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// SQLite returned an error while operating on the archive.
    #[error("SQLite operation failed: {0}")]
    Database(#[from] sqlx::Error),
    /// An embedded SQLite migration failed.
    #[error("archive migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

impl StoreError {
    /// Returns the stable machine-readable classification used by CLI JSON errors.
    pub fn code(&self) -> &'static str {
        match self {
            Self::AlreadyExists(_) => "archive_exists",
            Self::MissingArchive(_) => "archive_missing",
            Self::NotAFile(_) => "archive_not_file",
            Self::MetadataMissing => "archive_metadata_missing",
            Self::UnsupportedFormat(_) => "archive_format_unsupported",
            Self::MigrationHistoryMissing => "archive_migration_history_missing",
            Self::MigrationHistoryDirty { .. } => "archive_migration_history_dirty",
            Self::MigrationVersionUnknown { .. } => "archive_migration_unknown",
            Self::MigrationChecksumMismatch { .. } => "archive_migration_checksum_mismatch",
            Self::MigrationRequired { .. } => "archive_migration_required",
            Self::SchemaTooNew { .. } => "archive_schema_too_new",
            Self::InvalidArchiveId(_) => "archive_id_invalid",
            Self::InvalidCreatedAt(_) => "archive_timestamp_invalid",
            Self::ClockOutOfRange => "system_clock_out_of_range",
            Self::Io { .. } => "archive_io_error",
            Self::Database(_) => "archive_database_error",
            Self::Migration(_) => "archive_migration_error",
        }
    }
}
