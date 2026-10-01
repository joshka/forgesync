//! Independently acquired child families: comments, pull-request metadata, reviews, and review
//! threads.
//!
//! A collection reserves a sequence before provider I/O, stages pages, then finishes with a
//! validated page set. An incomplete finish records coverage without replacing the last complete
//! membership, which is why child writes are not folded into the parent observation transaction.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::{CommitSha, ObservationSequence, ThreadId};
use forgesync_core::observation::{CollectionCompleteness, SourceClock};
use forgesync_core::timestamp::UtcTimestamp;

use crate::observations::StagedItem;

/// Declares the child evidence to acquire before any provider request begins.
///
/// `source_clock` is the provider revision; `started_at` is local acquisition time.
#[derive(Clone, Copy)]
pub struct ChildFamilyRequest<'a> {
    /// Existing parent discussion whose independently covered children will be acquired.
    pub thread: &'a ThreadId,
    /// Child family; parent-thread evidence cannot use this reservation path.
    pub family: EvidenceFamily,
    /// Provider revision associated with this acquisition, not the local request start time.
    pub source_clock: &'a SourceClock,
    /// Local acquisition start recorded when the archive allocates its sequence.
    pub started_at: UtcTimestamp,
    /// Nonempty provider-request description used to distinguish the acquisition scope.
    pub request_scope: &'a str,
}

/// One provisional provider page belonging to a reserved child acquisition.
///
/// `page_index` follows provider traversal order from zero. Empty pages are valid; only the
/// terminal observation declares whether traversal completed.
pub struct ChildFamilyPage<'a, T> {
    /// Existing parent whose reserved collection receives this page.
    pub thread: &'a ThreadId,
    /// Independently acquired family selected during reservation.
    pub family: EvidenceFamily,
    /// Accepted acquisition sequence, not a newly allocated page sequence.
    pub sequence: ObservationSequence,
    /// Zero-based traversal position, used to detect missing pages and conflicting replay.
    pub page_index: u32,
    /// Provider identities and normalized payloads retained provisionally for finalization.
    pub items: &'a [StagedItem<T>],
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
pub(crate) mod query;
mod reservation;
mod staging;
