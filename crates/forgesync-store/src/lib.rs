#![forbid(unsafe_code)]

//! Durable archive operations and local query primitives.

mod archive;
mod enumeration;
mod error;
mod families;
mod health;
mod migration;
mod observations;
mod ordering;
mod reads;

pub use archive::{ARCHIVE_FORMAT_ID, Archive, ArchiveInfo};
pub use enumeration::{RepositoryThreadScan, RepositoryThreadScanStatus};
pub use error::StoreError;
pub use health::{DoctorReport, HealthCheck};
pub use migration::{AppliedMigration, MigrationReport};
pub use observations::{
    FamilyObservationResult, FamilyReservation, ObservationDisposition, StagedItem,
    ThreadObservationResult,
};
pub use ordering::{
    compare_observation_order, compare_revision_observation_order, observation_sequence_order_value,
};
pub use reads::{
    ArchiveStatus, FamilyCoverageSummary, ThreadDetail, ThreadPage, ThreadQuery, ThreadSort,
    ThreadStateFilter, ThreadSummary, ThreadTimelineEntry, ThreadTimelineEvent,
};
