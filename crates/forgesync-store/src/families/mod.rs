//! # Independent child-resource collection
//!
//! Comments, reviews, and review threads are acquired separately from their parent discussion.
//! [`ChildFamilyRequest`] declares the scope and clocks before acquisition. Reservation returns
//! the durable sequence used by every staged page and terminal observation for that attempt.
//! [`ChildFamilyPage`] pairs that generation with one page index and its provisional members.
//! `ChildFamilyObservation` identifies the parent, family, reserved sequence, acquisition time,
//! completeness, and optional review head. The engine constructs it after provider acquisition
//! reaches a terminal state.
//!
//! A collection reserves a sequence, stages pages, then finishes with a validated page set.
//! `reservation` owns the first step, `staging` owns page data, `finish` applies the terminal
//! result, and `query` reads collection state. A partial fetch can record its failure and coverage
//! without replacing complete canonical membership. That boundary is why child-family writes are
//! not folded into the parent observation transaction.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::{CommitSha, ObservationSequence, ThreadId};
use forgesync_core::observation::{CollectionCompleteness, SourceClock};
use forgesync_core::timestamp::UtcTimestamp;

use crate::observations::StagedItem;

/// Declares the child evidence to acquire before any provider request begins.
///
/// Pass this to
/// [`Archive::reserve_child_family_observation`](crate::archive::Archive::reserve_child_family_observation)
/// or its fenced counterpart. Reservation validates the declaration and allocates local ordering;
/// it does not fetch data, replace members, or declare complete coverage. Use the returned sequence
/// for staging and a [`ChildFamilyObservation`] when acquisition ends.
///
/// Source freshness and local acquisition time remain independent. A repeated source clock can
/// still receive a newer sequence, while a stale source generation can be rejected. The writer
/// lease is supplied separately because it authorizes persistence rather than describing evidence.
///
/// # Example
///
/// Reserve comment acquisition for an already stored discussion. Only a successful reservation
/// should proceed to provider I/O; rejected generations still consume local acquisition order.
///
/// ```no_run
/// use forgesync_core::coverage::EvidenceFamily;
/// use forgesync_core::identity::ThreadId;
/// use forgesync_core::observation::SourceClock;
/// use forgesync_core::timestamp::UtcTimestamp;
/// use forgesync_store::archive::Archive;
/// use forgesync_store::families::ChildFamilyRequest;
///
/// # async fn reserve(archive: &Archive, thread: &ThreadId, clock: &SourceClock,
/// # started_at: UtcTimestamp) -> Result<(), forgesync_store::error::StoreError> {
/// let request = ChildFamilyRequest {
///     thread,
///     family: EvidenceFamily::Comments,
///     source_clock: clock,
///     started_at,
///     request_scope: "comments page 1",
/// };
/// let reservation = archive.reserve_child_family_observation(request).await?;
/// if !reservation.reserved {
///     return Ok(());
/// }
/// // Fetch pages outside a transaction, then stage them using reservation.sequence.
/// # Ok(())
/// # }
/// ```
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

/// One durable page decoded for generation validation and canonical membership application.
///
/// The page index preserves pagination order; items retain provider identities so application
/// can detect duplicate members across pages before replacing the previous complete collection.
struct StagedPage {
    /// Zero-based position within the reserved generation.
    index: i64,
    /// Provider identities and payloads awaiting family-specific decoding.
    items: Vec<StagedItem<serde_json::Value>>,
}

/// One provisional provider page belonging to a previously reserved child acquisition.
///
/// Pass this to
/// [`Archive::stage_child_family_page`](crate::archive::Archive::stage_child_family_page)
/// or its fenced counterpart after fetching the page outside a database transaction. The parent,
/// family, and sequence must identify the accepted reservation. `page_index` follows provider
/// traversal order, beginning at zero. Empty slices are valid staged pages; the terminal
/// observation, not any individual page, declares whether traversal completed.
///
/// Staging serializes the items but does not publish canonical membership. Repeating the same page
/// succeeds only with the same serialized payload. Finish with [`ChildFamilyObservation`] once
/// acquisition is complete or interrupted; incomplete completion preserves prior complete members.
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

mod application;
mod finish;
mod query;
mod reservation;
mod staging;
