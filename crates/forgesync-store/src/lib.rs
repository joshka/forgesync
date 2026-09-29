#![forbid(unsafe_code)]

//! # Forgesync's durable, local archive
//!
//! The store is the boundary between domain observations and SQLite. `Archive` is the entry point:
//! callers explicitly create, open, or migrate a database, then pass the opened handle to
//! operations. Opening is deliberately free of acquisition and schema changes, so a local read
//! cannot unexpectedly contact GitHub or rewrite the archive.
//!
//! The modules follow the lifetime of data. `observations` accepts ordered parent snapshots;
//! `families` stages and completes independently acquired child resources. `runs`, `checkpoints`,
//! and `leases` record work that can be inspected and retried. `reads`, `documents`, `embeddings`,
//! and `clusters` serve offline discovery and local decisions. `diagnostics` and `health` expose
//! repairable state without concealing partial work.
//!
//! Domain types come from `forgesync_core`; SQL rows and conversion rules remain here. The engine
//! coordinates provider calls outside transactions and invokes these explicit archive methods only
//! when it has an observation or local decision to persist.
//!
//! # Choose the archive lifetime
//!
//! Use [`archive::Archive::create`] for a new database, [`archive::Archive::open_read_only`] for
//! inspection, and [`archive::Archive::open_read_write`] for an existing compatible database that
//! will receive writes. Migration is an explicit operation in [`migration`]; opening an old archive
//! does not silently upgrade it. Close the handle after its readers and workflows finish.
//!
//! # Inspect an existing archive
//!
//! This example performs no provider request and does not create a missing database. The counts and
//! coverage describe stored evidence; they do not imply that GitHub has been checked recently.
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
//!
//! # Write through observations
//!
//! Acquisition writes follow [`observations`] and [`families`], rather than ad hoc row replacement.
//! Reserve ordering before provider I/O, stage provisional child pages, and finalize completeness
//! explicitly. A failed or incomplete collection preserves earlier complete membership. Coordinated
//! writers use [`leases`] to fence every durable phase; the engine manages that workflow.
//!
//! Rust APIs are still evolving. Ordered migrations and observation semantics are deliberate
//! archive contracts; implementation types and module paths are not a promise of a stable external
//! SDK.

pub mod archive;
mod checkpoints;
pub mod clusters;
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
pub mod ordering;
pub mod reads;
pub mod runs;
