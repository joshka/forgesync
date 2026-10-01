#![forbid(unsafe_code)]

//! Forgesync's local SQLite archive.
//!
//! [`archive::Archive`] is the entry point: callers explicitly create, open, or migrate a
//! database, then pass the opened handle to operations. Opening never contacts GitHub or changes
//! the schema. Acquisition reserves ordering before provider I/O, stages child pages, and finalizes
//! completeness explicitly, so a failed or incomplete collection preserves earlier complete
//! membership. Coordinated writers fence every durable phase with [`leases`].
//!
//! ```no_run
//! use forgesync_store::archive::Archive;
//!
//! # async fn inspect() -> Result<(), forgesync_store::error::StoreError> {
//! let archive = Archive::open_read_only("archive.sqlite3").await?;
//! let status = archive.archive_status().await?;
//! println!(
//!     "{} repositories, {} discussions",
//!     status.repositories, status.threads
//! );
//! archive.close().await;
//! # Ok(())
//! # }
//! ```

pub mod archive;
pub mod checkpoints;
mod clock;
pub mod clusters;
mod coverage_projection;
pub mod diagnostics;
pub mod documents;
pub mod embeddings;
pub mod enumeration;
pub mod error;
pub mod families;
pub mod health;
pub mod leases;
pub mod migration;
pub mod observations;
mod ordering;
pub mod reads;
pub mod runs;
mod sql;
