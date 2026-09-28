#![forbid(unsafe_code)]

//! Durable archive operations and local query primitives.

mod archive;
mod error;
mod health;
mod migration;

pub use archive::{ARCHIVE_FORMAT_ID, Archive, ArchiveInfo};
pub use error::StoreError;
pub use health::{DoctorReport, HealthCheck};
pub use migration::{AppliedMigration, MigrationReport};
