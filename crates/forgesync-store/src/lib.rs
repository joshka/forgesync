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
