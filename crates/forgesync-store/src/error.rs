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
    /// A mutation was requested through a read-only archive handle.
    #[error("archive is open read-only")]
    ReadOnlyArchive,
    /// A provider repository identity has not been stored in this archive.
    #[error("repository is not present in the archive")]
    RepositoryMissing,
    /// A provider discussion identity has not been stored in this archive.
    #[error("thread is not present in the archive")]
    ThreadMissing,
    /// The family is not supported by the selected observation operation.
    #[error("unsupported observation family: {0}")]
    UnsupportedObservationFamily(String),
    /// The observation family does not match the operation being applied.
    #[error("observation family does not match the operation")]
    ObservationFamilyMismatch,
    /// A provider identity or observation sequence does not fit SQLite's integer range.
    #[error("observation identity or sequence is outside the SQLite integer range")]
    IntegerOutOfRange,
    /// A current row has an invalid or missing observation sequence.
    #[error("archive observation sequence is invalid")]
    InvalidStoredSequence,
    /// The provider supplied a source clock inconsistent with its declared state.
    #[error("source clock is invalid: {0}")]
    InvalidSourceClock(String),
    /// Distinct malformed source clocks cannot be ordered safely.
    #[error("ambiguous malformed observation timestamps {incoming:?} and {current:?}")]
    AmbiguousObservationClocks { incoming: String, current: String },
    /// Two different payloads claim the same source generation and sequence.
    #[error("conflicting observations share source generation and sequence")]
    ConflictingObservation,
    /// A collection generation was not reserved before staging or finalization.
    #[error("observation generation is not reserved")]
    ObservationGenerationMissing,
    /// A delayed collection page belongs to a generation superseded by a newer reservation.
    #[error("observation generation was superseded by a newer reservation")]
    StaleObservationGeneration,
    /// A repeated page number contained a different payload.
    #[error("replayed observation page conflicts with its previously staged payload")]
    StagedPageConflict,
    /// The same provider item ID appeared with different payloads in one collection.
    #[error("staged collection contains conflicting values for one provider ID")]
    StagedItemConflict,
    /// A completed collection did not stage the expected consecutive pages.
    #[error("complete collection expected {expected} pages but found {found}")]
    IncompletePageSet { expected: u32, found: u32 },
    /// A completion result did not supply the page count required for atomic application.
    #[error("complete collection requires an expected page count")]
    MissingExpectedPageCount,
    /// Review evidence was completed without identifying the pull-request head it describes.
    #[error("complete review evidence requires a pull-request head SHA")]
    MissingPullRequestHeadContext,
    /// A pull-request head context was attached to a non-review evidence family.
    #[error("pull-request head context is not valid for this evidence family")]
    UnexpectedPullRequestHeadContext,
    /// The collection completeness fields contradict the staged result.
    #[error("collection completeness does not match staged items or page count")]
    InvalidCollectionCompleteness,
    /// A repository thread scan was replaced by a newer acquisition sequence.
    #[error("repository thread scan was superseded by a newer acquisition")]
    StaleRepositoryThreadScan,
    /// No in-progress repository thread scan matches the supplied identity and sequence.
    #[error("repository thread scan is not in progress for this sequence")]
    RepositoryThreadScanMissing,
    /// A stored repository thread scan status is invalid.
    #[error("archive contains an invalid repository thread scan")]
    InvalidRepositoryThreadScan,
    /// Another sync operation currently owns the archive write lease.
    #[error("another sync operation currently owns the archive lease")]
    ArchiveLeaseHeld,
    /// The current sync operation no longer owns the archive write lease.
    #[error("archive write lease was lost or expired")]
    ArchiveLeaseLost,
    /// The archive lease duration must be positive and representable.
    #[error("archive lease duration is invalid")]
    InvalidArchiveLeaseDuration,
    /// A stored run, job, or outcome state is invalid.
    #[error("archive contains invalid sync run data")]
    InvalidRunData,
    /// A derived retrieval document has an unsupported recipe or invalid content hash.
    #[error("retrieval document data is invalid")]
    InvalidDocument,
    /// A stored or returned embedding has invalid identity, dimensions, or vector data.
    #[error("embedding data is invalid")]
    InvalidEmbedding,
    /// The source document changed before the embedding could be persisted.
    #[error("embedding source document is no longer current")]
    DocumentNotCurrent,
    /// The requested run is not present in this archive.
    #[error("sync run is not present in this archive")]
    RunMissing,
    /// A stored run, job, or checkpoint count is invalid.
    #[error("archive contains an invalid sync count")]
    InvalidSyncCount,
    /// The request scope is empty or contains only whitespace.
    #[error("observation request scope is required")]
    MissingRequestScope,
    /// The supplied coverage state cannot be persisted by the current observation operation.
    #[error("coverage state is not valid for this observation operation")]
    InvalidCoverageState,
    /// A coverage or staging result could not be encoded or decoded.
    #[error("observation JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    /// A stored provider ID is malformed.
    #[error("archive contains an invalid provider ID")]
    InvalidStoredProviderId,
    /// A stored coverage row has an unsupported family or state.
    #[error("archive contains an invalid coverage row")]
    InvalidStoredCoverage,
    /// A stored thread kind is unsupported by this binary.
    #[error("archive contains an unsupported thread kind: {0}")]
    InvalidStoredThreadKind(String),
    /// A stored count is negative or outside the supported range.
    #[error("archive contains an invalid count")]
    InvalidStoredCount,
    /// A generated cluster input violates identity, membership, or score invariants.
    #[error("generated cluster data is invalid")]
    InvalidClusterGeneration,
    /// A selected local cluster does not exist in this archive.
    #[error("cluster is not present in this archive")]
    ClusterMissing,
    /// A selected thread is not a current member of the cluster.
    #[error("thread is not a current cluster member")]
    ClusterMemberMissing,
    /// An advanced FTS5 query is malformed.
    #[error("advanced FTS5 search query is invalid")]
    InvalidSearchQuery,
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
            Self::ReadOnlyArchive => "archive_read_only",
            Self::RepositoryMissing => "repository_missing",
            Self::ThreadMissing => "thread_missing",
            Self::UnsupportedObservationFamily(_) => "observation_family_unsupported",
            Self::ObservationFamilyMismatch => "observation_family_mismatch",
            Self::IntegerOutOfRange => "observation_integer_out_of_range",
            Self::InvalidStoredSequence => "observation_sequence_invalid",
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
            Self::InvalidCoverageState => "coverage_state_invalid",
            Self::Json(_) => "observation_json_error",
            Self::InvalidStoredProviderId => "archive_provider_id_invalid",
            Self::InvalidStoredCoverage => "archive_coverage_invalid",
            Self::InvalidStoredThreadKind(_) => "archive_thread_kind_invalid",
            Self::InvalidStoredCount => "archive_count_invalid",
            Self::InvalidClusterGeneration => "cluster_generation_invalid",
            Self::ClusterMissing => "cluster_missing",
            Self::ClusterMemberMissing => "cluster_member_missing",
            Self::InvalidSearchQuery => "search_query_invalid",
            Self::Io { .. } => "archive_io_error",
            Self::Database(_) => "archive_database_error",
            Self::Migration(_) => "archive_migration_error",
        }
    }
}
