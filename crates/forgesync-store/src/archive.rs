//! Archive lifecycle: explicit create, read-only open, writable open, and migration.
//!
//! Opening never creates, migrates, or refreshes an archive. SQLite pragmas and pool settings live
//! here because they affect every transaction.

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
use crate::sql::timestamp_from_sql;

/// Format identity stored in each native Forgesync archive.
pub const ARCHIVE_FORMAT_ID: &str = "forgesync";

/// Bounds concurrent local reads without giving each query an unbounded SQLite connection.
const READER_CONNECTIONS: u32 = 4;
/// Bounds SQLite lock waiting for both read and write connections; this is not a workflow timeout.
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
    pub(crate) reader: SqlitePool,
    /// Present only for writable handles; mutations fail with `ReadOnlyArchive` without it.
    pub(crate) writer: Option<SqlitePool>,
    path: PathBuf,
    /// Metadata captured when this handle opened.
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

    /// Migrates and stamps metadata on a freshly created file, returning the opened handle.
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

            let info = validate_and_load_info(path, &writer, true).await?;
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
        let open_result = validate_and_load_info(&path, &reader, true).await;
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
            let info = validate_and_load_info(&path, &writer, true).await?;
            let reader = connect_reader(&path).await?;
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
            let info = validate_and_load_info(&path, &writer, false).await?;
            apply_pending_migrations(&writer, info.schema_version).await
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

    /// Closes all pools owned by this archive.
    pub async fn close(self) {
        self.reader.close().await;
        if let Some(writer) = self.writer {
            writer.close().await;
        }
    }
}

/// Checks archive metadata and migration history, optionally requiring the current schema.
async fn validate_and_load_info(
    path: &Path,
    pool: &SqlitePool,
    require_current_schema: bool,
) -> Result<ArchiveInfo, StoreError> {
    let has_metadata_table: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'archive_meta')",
    )
    .fetch_one(pool)
    .await?;
    if !has_metadata_table {
        return Err(StoreError::MetadataMissing);
    }
    let row = sqlx::query(
        "SELECT archive_id, format_id, created_at_us FROM archive_meta WHERE singleton = 1",
    )
    .fetch_optional(pool)
    .await?
    .ok_or(StoreError::MetadataMissing)?;
    let archive_id: String = row.try_get("archive_id")?;
    Uuid::parse_str(&archive_id).map_err(|_| StoreError::Corrupt("archive_id_invalid"))?;
    let format_id: String = row.try_get("format_id")?;
    if format_id != ARCHIVE_FORMAT_ID {
        return Err(StoreError::UnsupportedFormat(format_id));
    }
    let created_at = timestamp_from_sql(row.try_get("created_at_us")?)?;

    let schema_version = current_schema_version(pool).await?;
    validate_migration_history(pool, schema_version).await?;
    let supported = supported_schema_version();
    if require_current_schema && schema_version < supported {
        return Err(StoreError::MigrationRequired {
            current: schema_version,
            supported,
        });
    }
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

/// Rejects absent paths and non-file paths before opening SQLite.
fn existing_archive_file(path: &Path) -> Result<PathBuf, StoreError> {
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
