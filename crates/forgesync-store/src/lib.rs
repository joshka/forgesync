#![forbid(unsafe_code)]

//! Durable archive operations and local query primitives.
//!
//! [`archive`] controls explicit creation, opening, migration, and closure. [`observations`] and
//! [`families`] apply acquired content under the ordering and completeness rules. [`reads`] serves
//! offline inspection and search, while [`runs`] and [`leases`] track resumable work. [`clusters`]
//! stores local maintainer decisions. Callers open an archive explicitly and pass it to the
//! operation they need; an open does not initiate acquisition.

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
