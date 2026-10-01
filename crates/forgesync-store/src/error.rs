//! Typed store failures and their stable machine-readable codes.

use std::path::PathBuf;

use thiserror::Error;

/// Typed lifecycle, contract, authority, and persistence failures from archive operations.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("archive already exists: {0}")]
    AlreadyExists(PathBuf),
    #[error("archive does not exist: {0}")]
    MissingArchive(PathBuf),
    #[error("archive path is not a regular file: {0}")]
    NotAFile(PathBuf),
    #[error("archive metadata is missing")]
    MetadataMissing,
    #[error("unsupported archive format: {0}")]
    UnsupportedFormat(String),
    #[error("archive migration history is missing")]
    MigrationHistoryMissing,
    #[error("archive migration {version} is marked as incomplete")]
    MigrationHistoryDirty {
        /// Migration left incomplete on disk.
        version: i64,
    },
    #[error("archive migration {version} is not present in this binary")]
    MigrationVersionUnknown {
        /// Recorded migration absent from this binary.
        version: i64,
    },
    #[error("archive migration {version} does not match the embedded migration")]
    MigrationChecksumMismatch {
        /// Migration whose stored checksum changed.
        version: i64,
    },
    #[error("archive schema version {current} requires migration to version {supported}")]
    MigrationRequired {
        /// Version currently stored on disk.
        current: i64,
        /// Version supported by this binary.
        supported: i64,
    },
    #[error("archive schema version {found} is newer than supported version {supported}")]
    SchemaTooNew {
        /// Version found on disk.
        found: i64,
        /// Newest version this binary supports.
        supported: i64,
    },
    #[error("archive timestamp is invalid")]
    InvalidTimestamp(#[source] forgesync_core::timestamp::TimestampError),
    #[error("system clock is before the Unix epoch or outside the supported range")]
    ClockOutOfRange,
    #[error("archive is open read-only")]
    ReadOnlyArchive,
    #[error("repository is not present in the archive")]
    RepositoryMissing,
    #[error("thread is not present in the archive")]
    ThreadMissing,
    #[error("unsupported observation family: {0}")]
    UnsupportedObservationFamily(String),
    #[error("observation identity or sequence is outside the SQLite integer range")]
    IntegerOutOfRange,
    #[error("source clock is invalid: {0}")]
    InvalidSourceClock(String),
    #[error("ambiguous malformed observation timestamps {incoming:?} and {current:?}")]
    AmbiguousObservationClocks {
        /// Malformed timestamp on the incoming observation.
        incoming: String,
        /// Malformed timestamp on the current observation.
        current: String,
    },
    #[error("conflicting observations share source generation and sequence")]
    ConflictingObservation,
    #[error("observation generation is not reserved")]
    ObservationGenerationMissing,
    #[error("observation generation was superseded by a newer reservation")]
    StaleObservationGeneration,
    #[error("replayed observation page conflicts with its previously staged payload")]
    StagedPageConflict,
    #[error("staged collection contains conflicting values for one provider ID")]
    StagedItemConflict,
    #[error("complete collection expected {expected} pages but found {found}")]
    IncompletePageSet {
        /// Page count declared by the completed collection.
        expected: u32,
        /// Consecutive pages found in staging.
        found: u32,
    },
    #[error("complete collection requires an expected page count")]
    MissingExpectedPageCount,
    #[error("complete review evidence requires a pull-request head SHA")]
    MissingPullRequestHeadContext,
    #[error("pull-request head context is not valid for this evidence family")]
    UnexpectedPullRequestHeadContext,
    #[error("collection completeness does not match staged items or page count")]
    InvalidCollectionCompleteness,
    #[error("repository thread scan was superseded by a newer acquisition")]
    StaleRepositoryThreadScan,
    #[error("repository thread scan is not in progress for this sequence")]
    RepositoryThreadScanMissing,
    #[error("archive contains an invalid repository thread scan")]
    InvalidRepositoryThreadScan,
    #[error("another sync operation currently owns the archive lease")]
    ArchiveLeaseHeld,
    #[error("archive write lease was lost or expired")]
    ArchiveLeaseLost,
    #[error("archive lease duration is invalid")]
    InvalidArchiveLeaseDuration,
    #[error("archive contains invalid sync run data")]
    InvalidRunData,
    #[error("retrieval document data is invalid")]
    InvalidDocument,
    #[error("embedding data is invalid")]
    InvalidEmbedding,
    #[error("embedding source document is no longer current")]
    DocumentNotCurrent,
    #[error("sync run is not present in this archive")]
    RunMissing,
    #[error("archive contains an invalid sync count")]
    InvalidSyncCount,
    #[error("observation request scope is required")]
    MissingRequestScope,
    #[error("observation JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("generated cluster data is invalid")]
    InvalidClusterGeneration,
    #[error("cluster is not present in this archive")]
    ClusterMissing,
    #[error("thread is not a current cluster member")]
    ClusterMemberMissing,
    #[error("advanced FTS5 search query is invalid")]
    InvalidSearchQuery,
    /// Persisted data violates its stored representation; the payload is the stable error code.
    #[error("archive contains invalid stored data ({0})")]
    Corrupt(&'static str),
    #[error("archive filesystem operation failed for {path}: {source}")]
    Io {
        /// Path involved in the operation.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
    #[error("SQLite operation failed: {0}")]
    Database(#[from] sqlx::Error),
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
            Self::InvalidTimestamp(_) => "archive_timestamp_invalid",
            Self::ClockOutOfRange => "system_clock_out_of_range",
            Self::ReadOnlyArchive => "archive_read_only",
            Self::RepositoryMissing => "repository_missing",
            Self::ThreadMissing => "thread_missing",
            Self::UnsupportedObservationFamily(_) => "observation_family_unsupported",
            Self::IntegerOutOfRange => "observation_integer_out_of_range",
            Self::InvalidSourceClock(_) => "observation_source_clock_invalid",
            Self::AmbiguousObservationClocks { .. } => "observation_clock_ambiguous",
            Self::ConflictingObservation => "observation_conflict",
            Self::ObservationGenerationMissing => "observation_generation_missing",
            Self::StaleObservationGeneration => "observation_generation_stale",
            Self::StagedPageConflict => "observation_page_conflict",
            Self::StagedItemConflict => "observation_item_conflict",
            Self::IncompletePageSet { .. } => "observation_pages_incomplete",
            Self::MissingExpectedPageCount => "observation_page_count_missing",
            Self::MissingPullRequestHeadContext => "pull_request_head_context_missing",
            Self::UnexpectedPullRequestHeadContext => "pull_request_head_context_unexpected",
            Self::InvalidCollectionCompleteness => "observation_completeness_invalid",
            Self::StaleRepositoryThreadScan => "repository_scan_stale",
            Self::RepositoryThreadScanMissing => "repository_scan_missing",
            Self::InvalidRepositoryThreadScan => "repository_scan_invalid",
            Self::ArchiveLeaseHeld => "archive_lease_held",
            Self::ArchiveLeaseLost => "archive_lease_lost",
            Self::InvalidArchiveLeaseDuration => "archive_lease_duration_invalid",
            Self::InvalidRunData => "sync_run_invalid",
            Self::InvalidDocument => "document_invalid",
            Self::InvalidEmbedding => "embedding_invalid",
            Self::DocumentNotCurrent => "embedding_document_not_current",
            Self::RunMissing => "sync_run_missing",
            Self::InvalidSyncCount => "sync_count_invalid",
            Self::MissingRequestScope => "observation_scope_missing",
            Self::Json(_) => "observation_json_error",
            Self::InvalidClusterGeneration => "cluster_generation_invalid",
            Self::ClusterMissing => "cluster_missing",
            Self::ClusterMemberMissing => "cluster_member_missing",
            Self::InvalidSearchQuery => "search_query_invalid",
            Self::Corrupt(code) => code,
            Self::Io { .. } => "archive_io_error",
            Self::Database(_) => "archive_database_error",
            Self::Migration(_) => "archive_migration_error",
        }
    }
}
