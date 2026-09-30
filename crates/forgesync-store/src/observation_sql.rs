//! # Observation values at the SQLite boundary
//!
//! This private module converts source clocks, acquisition sequences, identities, and coverage
//! into the column vocabulary shared by observation and child-family transactions. Public domain
//! observation/result types remain in `observations`; SQL representations stay here.
//!
//! [`SourceClockColumns`] preserves missing, valid, and invalid clock shapes. Checked integer
//! conversions reject values outside SQLite's signed range. Identity lookups require existing
//! canonical rows rather than creating repository or discussion records implicitly.
//!
//! Coverage persistence accepts complete or incomplete state and writes through the caller's
//! connection. Callers own reservation/ordering checks, lease authority, and transaction commit;
//! these helpers neither validate the whole workflow nor commit independently. Ordinary public
//! helper visibility is constrained by this private module rather than leaking SQL into the API.

use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::identity::{ObservationSequence, ThreadId};
use forgesync_core::observation::SourceClock;
use forgesync_core::timestamp::UtcTimestamp;
use sqlx::SqliteConnection;

use crate::error::StoreError;

/// Checked SQL representation of a source clock for ordering and coverage persistence.
///
/// Exactly one shape is valid: missing has no value, valid has microseconds and no raw text, and
/// invalid retains nonempty source spelling with no microseconds. Conversion helpers enforce this
/// relationship; callers must not invent column combinations. The private module hides this
/// representation because the public observation API accepts domain clocks rather than SQL
/// representations.
#[derive(Clone, Debug)]
pub struct SourceClockColumns {
    /// Stored discriminant: `missing`, `valid`, or `invalid`.
    pub state: &'static str,
    /// Original invalid spelling after trimming; empty for missing and valid clocks.
    pub raw: String,
    /// Comparable timestamp present only for a valid clock.
    pub unix_microseconds: Option<i64>,
}

/// Maps a domain family to its stable archive label.
pub fn evidence_family_name(family: EvidenceFamily) -> &'static str {
    match family {
        EvidenceFamily::Threads => "threads",
        EvidenceFamily::Comments => "comments",
        EvidenceFamily::PullRequestMetadata => "pull_request_metadata",
        EvidenceFamily::Reviews => "reviews",
        EvidenceFamily::ReviewThreads => "review_threads",
    }
}

/// Distinguishes independently paged children from parent discussion evidence.
pub fn is_child_family(family: EvidenceFamily) -> bool {
    matches!(
        family,
        EvidenceFamily::Comments
            | EvidenceFamily::PullRequestMetadata
            | EvidenceFamily::Reviews
            | EvidenceFamily::ReviewThreads
    )
}

/// Validates a provider source clock before replacement ordering.
pub fn normalize_source_clock(clock: &SourceClock) -> Result<SourceClock, StoreError> {
    match clock {
        SourceClock::Invalid(raw) if raw.trim().is_empty() => Ok(SourceClock::Missing),
        SourceClock::Invalid(raw) => Ok(SourceClock::Invalid(raw.trim().to_owned())),
        SourceClock::Valid(timestamp) => Ok(SourceClock::Valid(*timestamp)),
        SourceClock::Missing => Ok(SourceClock::Missing),
    }
}

/// Splits a checked source clock into persisted state and value columns.
pub fn source_clock_columns(clock: &SourceClock) -> Result<SourceClockColumns, StoreError> {
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
pub fn source_clock_from_columns(
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
pub fn checked_sequence(value: i64) -> Result<ObservationSequence, StoreError> {
    if value <= 0 {
        return Err(StoreError::InvalidStoredSequence);
    }
    let value = u64::try_from(value).map_err(|_| StoreError::IntegerOutOfRange)?;
    ObservationSequence::new(value).map_err(|_| StoreError::InvalidStoredSequence)
}

/// Checks a sequence before binding it to SQLite's signed integer range.
pub fn to_sql_sequence(sequence: ObservationSequence) -> Result<i64, StoreError> {
    i64::try_from(sequence.get()).map_err(|_| StoreError::IntegerOutOfRange)
}

/// Checks an unsigned provider number before storing it in SQLite.
pub fn sqlite_integer(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::IntegerOutOfRange)
}

/// Resolves the registered repository key for an observation transaction.
pub async fn repository_row_id(
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
pub async fn thread_row_id(
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
///
/// Parent and child application call this on their existing transaction connection after deciding
/// which observation may advance coverage. It accepts only complete/incomplete states and stores
/// their serialized detail alongside source-clock columns supplied by the application owner.
/// Acquisition time and sequence come from the state itself, keeping indexed columns and JSON
/// consistent. This adapter does not redo ordering, fence, or membership validation. It neither
/// commits nor replaces child members.
/// State, serialization, integer-conversion, and database failures propagate to the transaction
/// owner so coverage and the owner's other writes can roll back together.
pub async fn write_coverage(
    connection: &mut SqliteConnection,
    thread_row_id: i64,
    family: EvidenceFamily,
    clock: &SourceClockColumns,
    state: &CoverageState,
) -> Result<(), StoreError> {
    let (status, observed_at, sequence) = match state {
        CoverageState::Complete {
            observed_at,
            sequence,
            ..
        } => ("complete", *observed_at, *sequence),
        CoverageState::Incomplete {
            observed_at,
            sequence,
            ..
        } => ("incomplete", *observed_at, *sequence),
        _ => return Err(StoreError::InvalidCoverageState),
    };
    let state_json = serde_json::to_string(state)?;
    let sequence = to_sql_sequence(sequence)?;
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
    .bind(sequence)
    .bind(state_json)
    .execute(&mut *connection)
    .await?;
    Ok(())
}
