//! # Independent child-resource collection
//!
//! Comments, reviews, and review threads are acquired separately from their parent discussion.
//! `ChildFamilyObservation` identifies the parent, family, reserved sequence, acquisition time,
//! completeness, and optional review head. The engine constructs it after provider acquisition
//! reaches a terminal state.
//!
//! A collection reserves a sequence, stages pages, then finishes with a validated page set.
//! `reservation` owns the first step, `staging` owns page data, `finish` applies the terminal
//! result, and `query` reads collection state. A partial fetch can record its failure and coverage
//! without replacing complete canonical membership. That boundary is why child-family writes are
//! not folded into the parent observation transaction.

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
    FamilyReservation, StagedItem, checked_sequence, evidence_family_name, is_child_family,
    normalize_source_clock, source_clock_columns, source_clock_from_columns, thread_row_id,
    to_sql_sequence,
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

mod application;
mod finish;
mod query;
mod reservation;
mod staging;
