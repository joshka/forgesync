//! Conversions between domain values and SQLite columns shared by every store module.
//!
//! Lookups require existing rows rather than creating repository or discussion records implicitly.
//! Query fragments assume the `r` (repository) and `t` (thread) aliases and an existing predicate.

use forgesync_core::content::ThreadKind;
use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::identity::{ObservationSequence, RepositoryId, ThreadId};
use forgesync_core::observation::SourceClock;
use forgesync_core::timestamp::UtcTimestamp;
use sqlx::{QueryBuilder, Sqlite, SqliteConnection, SqliteExecutor};

use crate::error::StoreError;
use crate::reads::ThreadStateFilter;

/// Stable presentation order of every evidence family.
pub const ALL_FAMILIES: [EvidenceFamily; 5] = [
    EvidenceFamily::Threads,
    EvidenceFamily::Comments,
    EvidenceFamily::PullRequestMetadata,
    EvidenceFamily::Reviews,
    EvidenceFamily::ReviewThreads,
];

/// Maps a family to its stored label.
pub fn family_name(family: EvidenceFamily) -> &'static str {
    match family {
        EvidenceFamily::Threads => "threads",
        EvidenceFamily::Comments => "comments",
        EvidenceFamily::PullRequestMetadata => "pull_request_metadata",
        EvidenceFamily::Reviews => "reviews",
        EvidenceFamily::ReviewThreads => "review_threads",
    }
}

/// Parses a stored family label; callers choose the corruption code for their table.
pub fn parse_family(value: &str) -> Option<EvidenceFamily> {
    ALL_FAMILIES
        .into_iter()
        .find(|family| family_name(*family) == value)
}

/// Distinguishes independently paged children from parent discussion evidence.
pub fn is_child_family(family: EvidenceFamily) -> bool {
    family != EvidenceFamily::Threads
}

/// Families that only apply to pull requests.
pub fn is_pull_request_family(family: EvidenceFamily) -> bool {
    matches!(
        family,
        EvidenceFamily::PullRequestMetadata
            | EvidenceFamily::Reviews
            | EvidenceFamily::ReviewThreads
    )
}

/// Maps a discussion kind to its stored label.
pub fn thread_kind_name(kind: ThreadKind) -> &'static str {
    match kind {
        ThreadKind::Issue => "issue",
        ThreadKind::PullRequest => "pull_request",
    }
}

/// Checks an unsigned value before binding it to SQLite's signed integer range.
pub fn to_sql_integer(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::IntegerOutOfRange)
}

/// Checks a sequence before binding it to SQLite's signed integer range.
pub fn to_sql_sequence(sequence: ObservationSequence) -> Result<i64, StoreError> {
    to_sql_integer(sequence.get())
}

/// Rejects zero, negative, or otherwise invalid stored acquisition sequences.
pub fn sequence_from_sql(value: i64) -> Result<ObservationSequence, StoreError> {
    u64::try_from(value)
        .ok()
        .and_then(|value| ObservationSequence::new(value).ok())
        .ok_or(StoreError::Corrupt("observation_sequence_invalid"))
}

/// Rejects a negative stored count.
pub fn count_from_sql(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::Corrupt("archive_count_invalid"))
}

/// Converts stored microseconds to a checked UTC timestamp.
pub fn timestamp_from_sql(value: i64) -> Result<UtcTimestamp, StoreError> {
    UtcTimestamp::from_unix_microseconds(value).map_err(StoreError::InvalidTimestamp)
}

/// Resolves a registered repository to its row key.
pub async fn repository_row_id(
    connection: &mut SqliteConnection,
    repository: &RepositoryId,
) -> Result<i64, StoreError> {
    sqlx::query_scalar("SELECT id FROM repositories WHERE host = ? AND provider_id = ?")
        .bind(repository.host().as_str())
        .bind(repository.provider_id().as_str())
        .fetch_optional(connection)
        .await?
        .ok_or(StoreError::RepositoryMissing)
}

/// Looks up a stored discussion row key by its full identity.
pub async fn find_thread_row_id<'e>(
    executor: impl SqliteExecutor<'e>,
    thread: &ThreadId,
) -> Result<Option<i64>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT t.id FROM threads t JOIN repositories r ON r.id = t.repository_id WHERE r.host = ? AND r.provider_id = ? AND t.provider_id = ? AND t.number = ?",
    )
    .bind(thread.repository().host().as_str())
    .bind(thread.repository().provider_id().as_str())
    .bind(thread.provider_id().as_str())
    .bind(to_sql_integer(thread.number().get())?)
    .fetch_optional(executor)
    .await?)
}

/// Resolves a stored discussion row key, failing when the discussion is absent.
pub async fn thread_row_id(
    connection: &mut SqliteConnection,
    thread: &ThreadId,
) -> Result<i64, StoreError> {
    find_thread_row_id(connection, thread)
        .await?
        .ok_or(StoreError::ThreadMissing)
}

/// Appends `, `-separated bound values, e.g. inside `IN (...)`.
pub fn push_bound_list(statement: &mut QueryBuilder<Sqlite>, values: &[i64]) {
    let mut separated = statement.separated(", ");
    for value in values {
        separated.push_bind(*value);
    }
}

/// Adds bound repository identities; an empty scope adds no restriction.
pub fn push_repository_scope(statement: &mut QueryBuilder<Sqlite>, repositories: &[RepositoryId]) {
    if repositories.is_empty() {
        return;
    }
    statement.push(" AND (");
    for (index, repository) in repositories.iter().enumerate() {
        if index > 0 {
            statement.push(" OR ");
        }
        statement
            .push("(r.host = ")
            .push_bind(repository.host().as_str())
            .push(" AND r.provider_id = ")
            .push_bind(repository.provider_id().as_str())
            .push(")");
    }
    statement.push(")");
}

/// Adds kind and source-state predicates on the `t` alias.
pub fn push_discussion_filters(
    statement: &mut QueryBuilder<Sqlite>,
    kind: Option<ThreadKind>,
    state: ThreadStateFilter,
) {
    if let Some(kind) = kind {
        statement
            .push(" AND t.kind = ")
            .push_bind(thread_kind_name(kind));
    }
    match state {
        ThreadStateFilter::All => {}
        ThreadStateFilter::Open => {
            statement.push(" AND t.state = 'open'");
        }
        ThreadStateFilter::Closed => {
            statement.push(" AND t.state = 'closed'");
        }
    }
}

/// Stored source-clock columns.
///
/// Exactly one shape is valid: missing has no value, valid has microseconds and empty raw text,
/// and invalid retains its nonempty trimmed spelling without microseconds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceClockColumns {
    /// Stored discriminant: `missing`, `valid`, or `invalid`.
    pub state: &'static str,
    /// Trimmed invalid spelling; empty for missing and valid clocks.
    pub raw: String,
    /// Comparable timestamp, present only for a valid clock.
    pub unix_microseconds: Option<i64>,
}

/// Splits a source clock into its stored columns.
pub fn source_clock_columns(clock: &SourceClock) -> SourceClockColumns {
    match clock {
        SourceClock::Missing => SourceClockColumns {
            state: "missing",
            raw: String::new(),
            unix_microseconds: None,
        },
        SourceClock::Valid(timestamp) => SourceClockColumns {
            state: "valid",
            raw: String::new(),
            unix_microseconds: Some(timestamp.unix_microseconds()),
        },
        SourceClock::Invalid(raw) => SourceClockColumns {
            state: "invalid",
            raw: raw.clone(),
            unix_microseconds: None,
        },
    }
}

/// Reconstructs a source clock from stored columns.
pub fn source_clock_from_columns(
    state: &str,
    raw: &str,
    unix_microseconds: Option<i64>,
) -> Result<SourceClock, StoreError> {
    match (state, raw, unix_microseconds) {
        ("missing", "", None) => Ok(SourceClock::Missing),
        ("valid", "", Some(microseconds)) => {
            Ok(SourceClock::Valid(timestamp_from_sql(microseconds)?))
        }
        ("invalid", raw, None) if !raw.is_empty() => Ok(SourceClock::Invalid(raw.to_owned())),
        _ => Err(StoreError::InvalidSourceClock(
            "stored clock columns violate their state".to_owned(),
        )),
    }
}

/// Records a family's complete or incomplete coverage on the caller's transaction.
///
/// Acquisition time and sequence come from the state itself, keeping indexed columns and JSON
/// consistent. Ordering, fencing, and membership checks belong to the caller.
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
        _ => return Err(StoreError::Corrupt("coverage_state_invalid")),
    };
    sqlx::query(
        "INSERT INTO family_coverage (thread_id, family, status, source_clock_state, source_clock_raw, source_clock_us, observed_at_us, sequence, state_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (thread_id, family) DO UPDATE SET status = excluded.status, source_clock_state = excluded.source_clock_state, source_clock_raw = excluded.source_clock_raw, source_clock_us = excluded.source_clock_us, observed_at_us = excluded.observed_at_us, sequence = excluded.sequence, state_json = excluded.state_json",
    )
    .bind(thread_row_id)
    .bind(family_name(family))
    .bind(status)
    .bind(clock.state)
    .bind(&clock.raw)
    .bind(clock.unix_microseconds)
    .bind(observed_at.unix_microseconds())
    .bind(to_sql_sequence(sequence)?)
    .bind(serde_json::to_string(state)?)
    .execute(&mut *connection)
    .await?;
    Ok(())
}
