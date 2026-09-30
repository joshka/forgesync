//! # Ordered source observations and coverage
//!
//! A thread observation is evidence acquired at a particular local sequence with a source clock.
//! `ObservationDisposition` tells callers whether it became canonical; `ThreadObservationResult`
//! reports the application. `FamilyReservation`, `FamilyObservationResult`, and `StagedItem`
//! support child-family acquisition without conflating it with the parent snapshot.
//!
//! `sequence` allocates durable local order, `apply` commits a parent snapshot, `repository`
//! resolves its scope, and `coverage` reads directly recorded family completeness. Source update
//! time and acquisition order have different jobs: an older provider revision should not displace
//! newer canonical content merely because it arrived later. The engine decides when to fetch; the
//! store enforces these ordering and completeness rules.

use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::identity::{ObservationSequence, ProviderId, ThreadId};
use forgesync_core::observation::SourceClock;
use forgesync_core::timestamp::UtcTimestamp;
use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;

use crate::error::StoreError;

/// The disposition of an observation or family reservation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationDisposition {
    /// The observation changed canonical content, evidence, membership, or coverage.
    Applied,
    /// The same observation had already been applied.
    Replayed,
    /// A newer reservation or accepted generation made this observation stale.
    Skipped,
}

/// Result of applying one canonical thread observation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ThreadObservationResult {
    /// Stable row ID for use by later local queries.
    pub thread_row_id: i64,
    /// Whether canonical content or complete evidence changed.
    pub disposition: ObservationDisposition,
    /// Highest sequence observed for the current source generation.
    pub high_water_sequence: ObservationSequence,
    /// Sequence of the last accepted complete parent observation, if any.
    pub evidence_sequence: Option<ObservationSequence>,
}

/// One stable provider item staged for a child-family collection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StagedItem<T> {
    /// Provider-issued identity within this family and parent thread.
    pub id: ProviderId,
    /// Normalized item value serialized at the store boundary.
    pub payload: T,
}

/// Result of reserving a sequence for one independently refreshed family.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FamilyReservation {
    /// Sequence allocated before acquisition.
    pub sequence: ObservationSequence,
    /// False when a newer source generation or acquisition already owns the family reservation.
    pub reserved: bool,
}

/// Result of completing or recording an incomplete child-family collection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FamilyObservationResult {
    /// Whether this generation changed current membership or coverage.
    pub disposition: ObservationDisposition,
    /// Number of distinct provider items staged for this generation.
    pub item_count: u64,
}

/// Checked SQL representation of a source clock for ordering and coverage persistence.
///
/// Exactly one shape is valid: missing has no value, valid has microseconds and no raw text, and
/// invalid retains nonempty source spelling with no microseconds. Conversion helpers enforce this
/// relationship; callers must not invent column combinations. This remains crate-restricted
/// because the public observation API accepts domain clocks rather than SQL representations.
#[derive(Clone, Debug)]
pub(crate) struct SourceClockColumns {
    /// Stored discriminant: `missing`, `valid`, or `invalid`.
    pub state: &'static str,
    /// Original invalid spelling after trimming; empty for missing and valid clocks.
    pub raw: String,
    /// Comparable timestamp present only for a valid clock.
    pub unix_microseconds: Option<i64>,
}

mod apply;
mod coverage;
mod repository;
mod sequence;
mod thread_rows;

/// Maps a domain family to its stable archive label.
pub(crate) fn evidence_family_name(family: EvidenceFamily) -> &'static str {
    match family {
        EvidenceFamily::Threads => "threads",
        EvidenceFamily::Comments => "comments",
        EvidenceFamily::PullRequestMetadata => "pull_request_metadata",
        EvidenceFamily::Reviews => "reviews",
        EvidenceFamily::ReviewThreads => "review_threads",
    }
}

/// Distinguishes independently paged children from parent discussion evidence.
pub(crate) fn is_child_family(family: EvidenceFamily) -> bool {
    matches!(
        family,
        EvidenceFamily::Comments
            | EvidenceFamily::PullRequestMetadata
            | EvidenceFamily::Reviews
            | EvidenceFamily::ReviewThreads
    )
}

/// Validates a provider source clock before replacement ordering.
pub(crate) fn normalize_source_clock(clock: &SourceClock) -> Result<SourceClock, StoreError> {
    match clock {
        SourceClock::Invalid(raw) if raw.trim().is_empty() => Ok(SourceClock::Missing),
        SourceClock::Invalid(raw) => Ok(SourceClock::Invalid(raw.trim().to_owned())),
        SourceClock::Valid(timestamp) => Ok(SourceClock::Valid(*timestamp)),
        SourceClock::Missing => Ok(SourceClock::Missing),
    }
}

/// Splits a checked source clock into persisted state and value columns.
pub(crate) fn source_clock_columns(clock: &SourceClock) -> Result<SourceClockColumns, StoreError> {
    match clock {
        SourceClock::Missing => Ok(SourceClockColumns {
            state: "missing",
            raw: String::new(),
            unix_microseconds: None,
        }),
        SourceClock::Valid(timestamp) => Ok(SourceClockColumns {
            state: "valid",
            raw: String::new(),
            unix_microseconds: Some(timestamp.unix_microseconds()),
        }),
        SourceClock::Invalid(raw) if !raw.trim().is_empty() => Ok(SourceClockColumns {
            state: "invalid",
            raw: raw.trim().to_owned(),
            unix_microseconds: None,
        }),
        SourceClock::Invalid(_) => Err(StoreError::InvalidSourceClock(
            "an invalid clock must retain its source spelling".to_owned(),
        )),
    }
}

/// Reconstructs a source clock from checked stored columns.
pub(crate) fn source_clock_from_columns(
    state: &str,
    raw: &str,
    unix_microseconds: Option<i64>,
) -> Result<SourceClock, StoreError> {
    match (state, raw, unix_microseconds) {
        ("missing", "", None) => Ok(SourceClock::Missing),
        ("valid", "", Some(microseconds)) => Ok(SourceClock::Valid(
            UtcTimestamp::from_unix_microseconds(microseconds)
                .map_err(StoreError::InvalidCreatedAt)?,
        )),
        ("invalid", raw, None) if !raw.is_empty() => Ok(SourceClock::Invalid(raw.to_owned())),
        _ => Err(StoreError::InvalidSourceClock(
            "stored clock columns violate their state".to_owned(),
        )),
    }
}

/// Rejects zero or invalid stored acquisition sequences.
pub(crate) fn checked_sequence(value: i64) -> Result<ObservationSequence, StoreError> {
    if value <= 0 {
        return Err(StoreError::InvalidStoredSequence);
    }
    let value = u64::try_from(value).map_err(|_| StoreError::IntegerOutOfRange)?;
    ObservationSequence::new(value).map_err(|_| StoreError::InvalidStoredSequence)
}

/// Checks a sequence before binding it to SQLite's signed integer range.
pub(crate) fn to_sql_sequence(sequence: ObservationSequence) -> Result<i64, StoreError> {
    i64::try_from(sequence.get()).map_err(|_| StoreError::IntegerOutOfRange)
}

/// Checks an unsigned provider number before storing it in SQLite.
pub(crate) fn sqlite_integer(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::IntegerOutOfRange)
}

/// Resolves the registered repository key for an observation transaction.
pub(crate) async fn repository_row_id(
    connection: &mut SqliteConnection,
    host: &str,
    provider_id: &str,
) -> Result<i64, StoreError> {
    sqlx::query_scalar("SELECT id FROM repositories WHERE host = ? AND provider_id = ?")
        .bind(host)
        .bind(provider_id)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or(StoreError::RepositoryMissing)
}

/// Resolves the stored parent discussion key for child evidence.
pub(crate) async fn thread_row_id(
    connection: &mut SqliteConnection,
    thread: &ThreadId,
) -> Result<i64, StoreError> {
    let id = sqlx::query_scalar(
        "SELECT t.id FROM threads t JOIN repositories r ON r.id = t.repository_id WHERE r.host = ? AND r.provider_id = ? AND t.provider_id = ? AND t.number = ?",
    )
    .bind(thread.repository().host().as_str())
    .bind(thread.repository().provider_id().as_str())
    .bind(thread.provider_id().as_str())
    .bind(sqlite_integer(thread.number().get())?)
    .fetch_optional(&mut *connection)
    .await?;
    id.ok_or(StoreError::ThreadMissing)
}

/// Records a family's completeness separately from its current membership.
pub(crate) async fn write_coverage(
    connection: &mut SqliteConnection,
    thread_row_id: i64,
    family: EvidenceFamily,
    clock: &SourceClockColumns,
    observed_at: UtcTimestamp,
    sequence: ObservationSequence,
    state: &CoverageState,
) -> Result<(), StoreError> {
    let status = match state {
        CoverageState::Complete { .. } => "complete",
        CoverageState::Incomplete { .. } => "incomplete",
        _ => return Err(StoreError::InvalidCoverageState),
    };
    let state_json = serde_json::to_string(state)?;
    sqlx::query(
        "INSERT INTO family_coverage (thread_id, family, status, source_clock_state, source_clock_raw, source_clock_us, observed_at_us, sequence, state_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (thread_id, family) DO UPDATE SET status = excluded.status, source_clock_state = excluded.source_clock_state, source_clock_raw = excluded.source_clock_raw, source_clock_us = excluded.source_clock_us, observed_at_us = excluded.observed_at_us, sequence = excluded.sequence, state_json = excluded.state_json",
    )
    .bind(thread_row_id)
    .bind(evidence_family_name(family))
    .bind(status)
    .bind(clock.state)
    .bind(&clock.raw)
    .bind(clock.unix_microseconds)
    .bind(observed_at.unix_microseconds())
    .bind(to_sql_sequence(sequence)?)
    .bind(state_json)
    .execute(&mut *connection)
    .await?;
    Ok(())
}
