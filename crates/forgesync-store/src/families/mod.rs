//! Child resource-family observations and membership.
//!
//! Apply child resource families such as comments and reviews under their own completeness
//! boundary. Staging and finalization keep a partial provider page from replacing a previously
//! complete membership.

use std::collections::BTreeMap;

use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::identity::{CommitSha, ObservationSequence, ThreadId};
use forgesync_core::observation::{CollectionCompleteness, SourceClock};
use forgesync_core::timestamp::UtcTimestamp;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::SqliteConnection;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::observations::{
    FamilyObservationResult, FamilyReservation, ObservationDisposition, StagedItem,
    checked_sequence, evidence_family_name, is_child_family, normalize_source_clock,
    source_clock_columns, source_clock_from_columns, thread_row_id, to_sql_sequence,
    write_coverage,
};
use crate::ordering::compare_observation_order;

struct StagedPage {
    index: i64,
    items: Vec<StagedItem<serde_json::Value>>,
}

/// Inputs that identify and classify one finished child-family collection.
#[derive(Clone, Copy)]
pub struct ChildFamilyObservation<'a> {
    /// Parent discussion whose child family was acquired.
    pub thread: &'a ThreadId,
    /// Independently acquired evidence family.
    pub family: EvidenceFamily,
    /// Sequence reserved before provider acquisition.
    pub sequence: ObservationSequence,
    /// Local time when acquisition reached this terminal state.
    pub observed_at: UtcTimestamp,
    /// Whether all pages were acquired and validated.
    pub completeness: &'a CollectionCompleteness,
    /// Number of pages in a complete collection; omitted for incomplete results.
    pub expected_pages: Option<u32>,
    /// Pull-request head the completed review evidence describes.
    pub head_sha: Option<&'a CommitSha>,
}

mod finish;
mod query;
mod reservation;
mod staging;

use staging::{count_staged_items, load_staged_pages, merge_staged_items, validate_page_set};
