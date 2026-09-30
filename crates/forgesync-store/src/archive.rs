//! # Archive lifecycle and database handles
//!
//! `Archive` owns the SQLite pools used by every store operation. `ArchiveInfo` describes an
//! opened archive for status output. Create, read-only open, writable open, and migration are
//! separate calls so a command can choose its side effects deliberately.
//!
//! Use the read-only handle for inspection paths and a writable handle for observations, derived
//! data, or local decisions. Other modules add focused `impl Archive` methods; this file owns
//! connection setup and the rules shared by all of them. SQLite pragmas and pool behavior belong
//! here because they affect every transaction, including concurrency and foreign-key integrity.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::time::Duration;

use forgesync_core::timestamp::UtcTimestamp;
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::clock::now_utc;
use crate::error::StoreError;
use crate::migration::{
    MIGRATOR, MigrationReport, apply_pending_migrations, current_schema_version,
    supported_schema_version, validate_migration_history,
};

/// Format identity stored in each native Forgesync archive.
pub const ARCHIVE_FORMAT_ID: &str = "forgesync";

const READER_CONNECTIONS: u32 = 4;
const WRITER_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Stable archive metadata returned by `status` and lifecycle operations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ArchiveInfo {
    /// Archive file location as a display-safe string.
    pub path: String,
    /// Stable UUID assigned when the archive is created.
    pub archive_id: String,
    /// Native archive format family.
    pub format_id: String,
    /// Archive creation time normalized to UTC microseconds.
    pub created_at: UtcTimestamp,
    /// Highest successfully applied schema migration.
    pub schema_version: i64,
    /// SQLite library version reported by the active connection.
    pub sqlite_version: String,
}

/// Open archive pools. Reads use a read-only pool; writes use a serialized writer pool.
///
/// Lifecycle operations are explicit: [`Self::create`] creates a new file, the two `open` methods
/// validate an existing file, and [`Self::migrate`] upgrades an older schema. Opening never
/// acquires provider data or applies pending migrations. Keep a handle open while running local
/// reads or workflows, and call [`Self::close`] for orderly pool shutdown.
///
/// # Examples
///
/// ```no_run
/// use forgesync_store::archive::Archive;
///
/// # async fn example() -> Result<(), forgesync_store::error::StoreError> {
/// let archive = Archive::open_read_only("archive.sqlite").await?;
/// let info = archive.info();
/// println!(
///     "archive {} uses schema {}",
///     info.archive_id, info.schema_version
/// );
/// archive.close().await;
/// # Ok(())
/// # }
/// ```
pub struct Archive {
    /// Validated read pool shared by store operations, hidden from archive consumers.
    pub(crate) reader: SqlitePool,
    /// Writable capability present only for explicitly writable handles.
    ///
    /// Store operations check this capability before mutation; exposing the pool publicly would
    /// bypass lifecycle, fencing, and transaction APIs.
    pub(crate) writer: Option<SqlitePool>,
    path: PathBuf,
    info: ArchiveInfo,
}

impl Archive {
    /// Creates a new archive at `path`; an existing file is never overwritten.
    pub async fn create(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|source| {
                if source.kind() == std::io::ErrorKind::AlreadyExists {
                    StoreError::AlreadyExists(path.clone())
                } else {
                    StoreError::Io {
                        path: path.clone(),
                        source,
                    }
                }
            })?;
        drop(file);

        let result = Self::create_new_archive(&path).await;
        if result.is_err() {
            remove_archive_files(&path);
        }
        result
    }

    /// Initializes schema and metadata after exclusive file creation.
    async fn create_new_archive(path: &Path) -> Result<Self, StoreError> {
        let writer = connect_writer(path).await?;
        let setup_result = async {
            MIGRATOR.run(&writer).await?;
            let created_at = now_utc()?;
            let archive_id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO archive_meta (singleton, archive_id, format_id, created_at_us) VALUES (1, ?, ?, ?)",
            )
            .bind(&archive_id)
            .bind(ARCHIVE_FORMAT_ID)
            .bind(created_at.unix_microseconds())
            .execute(&writer)
            .await?;

            let schema_version = current_schema_version(&writer).await?;
            let info = load_info(path, &writer, schema_version).await?;
            let reader = connect_reader(path).await?;
            Ok::<_, StoreError>((reader, info))
        }
        .await;

        match setup_result {
            Ok((reader, info)) => Ok(Self {
                reader,
                writer: Some(writer),
                path: path.to_path_buf(),
                info,
            }),
            Err(error) => {
                writer.close().await;
                Err(error)
            }
        }
    }

    /// Opens an existing archive for local reads without creating or migrating it.
    pub async fn open_read_only(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = existing_archive_file(path.as_ref())?;
        let reader = connect_reader(&path).await?;
        let open_result = validate_and_load_info(&path, &reader).await;
        match open_result {
            Ok(info) => Ok(Self {
                reader,
                writer: None,
                path,
                info,
            }),
            Err(error) => {
                reader.close().await;
                Err(error)
            }
        }
    }

    /// Opens an existing archive for reads and serialized writes without creating or migrating it.
    pub async fn open_read_write(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = existing_archive_file(path.as_ref())?;
        let writer = connect_writer(&path).await?;
        let open_result = async {
            validate_and_load_info(&path, &writer).await?;
            let reader = connect_reader(&path).await?;
            let info = validate_and_load_info(&path, &reader).await?;
            Ok::<_, StoreError>((reader, info))
        }
        .await;

        match open_result {
            Ok((reader, info)) => Ok(Self {
                reader,
                writer: Some(writer),
                path,
                info,
            }),
            Err(error) => {
                writer.close().await;
                Err(error)
            }
        }
    }

    /// Applies pending embedded migrations to an existing archive.
    pub async fn migrate(path: impl AsRef<Path>) -> Result<MigrationReport, StoreError> {
        let path = existing_archive_file(path.as_ref())?;
        let writer = connect_writer(&path).await?;
        let result = async {
            load_metadata(&writer).await?;
            let previous_schema_version = current_schema_version(&writer).await?;
            validate_migration_history(&writer, previous_schema_version).await?;
            load_info(&path, &writer, previous_schema_version).await?;
            apply_pending_migrations(&writer, previous_schema_version).await
        }
        .await;
        writer.close().await;
        result
    }

    /// Returns the archive path used to open this handle.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns validated archive metadata captured when the handle was opened.
    pub fn info(&self) -> &ArchiveInfo {
        &self.info
    }

    /// Returns whether this handle can write to the archive.
    pub fn is_read_only(&self) -> bool {
        self.writer.is_none()
    }

    /// Closes all pools owned by this archive.
    pub async fn close(self) {
        self.reader.close().await;
        if let Some(writer) = self.writer {
            writer.close().await;
        }
    }
}

/// Checks archive format and migration history before exposing a handle.
///
/// Shared by lifecycle and diagnostics but crate-only because it accepts a raw pool. Consumers
/// choose explicit archive opening operations rather than validating arbitrary SQL resources.
pub(crate) async fn validate_and_load_info(
    path: &Path,
    pool: &SqlitePool,
) -> Result<ArchiveInfo, StoreError> {
    load_metadata(pool).await?;
    let current = current_schema_version(pool).await?;
    let supported = supported_schema_version();
    if current > supported {
        return Err(StoreError::SchemaTooNew {
            found: current,
            supported,
        });
    }
    validate_migration_history(pool, current).await?;
    if current < supported {
        return Err(StoreError::MigrationRequired { current, supported });
    }

    load_info(path, pool, current).await
}

/// Loads validated archive identity and SQLite metadata for status output.
async fn load_info(
    path: &Path,
    pool: &SqlitePool,
    schema_version: i64,
) -> Result<ArchiveInfo, StoreError> {
    let row = sqlx::query(
        "SELECT archive_id, format_id, created_at_us FROM archive_meta WHERE singleton = 1",
    )
    .fetch_optional(pool)
    .await?
    .ok_or(StoreError::MetadataMissing)?;
    let archive_id: String = row.try_get("archive_id")?;
    Uuid::parse_str(&archive_id).map_err(StoreError::InvalidArchiveId)?;
    let format_id: String = row.try_get("format_id")?;
    if format_id != ARCHIVE_FORMAT_ID {
        return Err(StoreError::UnsupportedFormat(format_id));
    }
    let created_at_us: i64 = row.try_get("created_at_us")?;
    let created_at = UtcTimestamp::from_unix_microseconds(created_at_us)
        .map_err(StoreError::InvalidCreatedAt)?;
    let sqlite_version: String = sqlx::query_scalar("SELECT sqlite_version()")
        .fetch_one(pool)
        .await?;

    Ok(ArchiveInfo {
        path: path.to_string_lossy().into_owned(),
        archive_id,
        format_id,
        created_at,
        schema_version,
        sqlite_version,
    })
}

/// Reads the singleton metadata row without creating it.
async fn load_metadata(pool: &SqlitePool) -> Result<(), StoreError> {
    let has_metadata_table: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'archive_meta')",
    )
    .fetch_one(pool)
    .await?;
    if has_metadata_table == 0 {
        return Err(StoreError::MetadataMissing);
    }

    let has_metadata_row: i64 =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM archive_meta WHERE singleton = 1)")
            .fetch_one(pool)
            .await?;
    if has_metadata_row == 0 {
        return Err(StoreError::MetadataMissing);
    }
    Ok(())
}

/// Rejects absent paths and non-file paths before opening SQLite.
///
/// This lifecycle precondition is shared with explicit migration, not a public archive-opening
/// alternative. It checks filesystem shape without creating, migrating, or validating schema.
pub(crate) fn existing_archive_file(path: &Path) -> Result<PathBuf, StoreError> {
    let metadata = std::fs::metadata(path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            StoreError::MissingArchive(path.to_path_buf())
        } else {
            StoreError::Io {
                path: path.to_path_buf(),
                source,
            }
        }
    })?;
    if !metadata.is_file() {
        return Err(StoreError::NotAFile(path.to_path_buf()));
    }
    Ok(path.to_path_buf())
}

/// Opens a read-only pool with archive-safe SQLite settings.
async fn connect_reader(path: &Path) -> Result<SqlitePool, StoreError> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .read_only(true)
        .foreign_keys(true)
        .busy_timeout(WRITER_BUSY_TIMEOUT);
    Ok(SqlitePoolOptions::new()
        .max_connections(READER_CONNECTIONS)
        .connect_with(options)
        .await?)
}

/// Opens the serialized writer pool with foreign keys enabled.
async fn connect_writer(path: &Path) -> Result<SqlitePool, StoreError> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .foreign_keys(true)
        .busy_timeout(WRITER_BUSY_TIMEOUT)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full);
    Ok(SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?)
}

/// Cleans up an incomplete newly created archive after setup fails.
fn remove_archive_files(path: &Path) {
    let _ = std::fs::remove_file(path);
    remove_sqlite_sidecar(path, "-wal");
    remove_sqlite_sidecar(path, "-shm");
}

/// Removes a SQLite sidecar left by failed archive creation.
fn remove_sqlite_sidecar(path: &Path, suffix: &str) {
    let mut sidecar = path.as_os_str().to_os_string();
    sidecar.push(suffix);
    let _ = std::fs::remove_file(PathBuf::from(sidecar));
}
