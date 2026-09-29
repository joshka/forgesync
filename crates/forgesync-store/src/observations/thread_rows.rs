//! # SQL rows for canonical discussion application
//!
//! These helpers translate normalized discussions and stored clocks at the transaction boundary.
//! [`IncomingThread::insert`] creates a row, [`load_thread_observation`] reconstructs the current
//! source and evidence positions, and [`update_thread_payload`] writes the selected canonical
//! snapshot.
//!
//! [`StoredThreadObservation`] retains separate canonical and complete-evidence positions for that
//! decision. [`ThreadPayloadUpdate`] carries the selected snapshot and optional evidence advance
//! into the update statement. Neither representation escapes the observation implementation.
//!
//! The application module owns ordering policy; this file owns column mapping and bound SQL.
//! The long insert/update binding sequences remain linear so a reader can compare field order
//! directly with the query. Every helper uses the caller's connection and never commits separately.

use forgesync_core::content::{Discussion, SourceState, ThreadKind};
use forgesync_core::identity::ObservationSequence;
use forgesync_core::observation::SourceClock;
use forgesync_core::timestamp::UtcTimestamp;
use sqlx::{Row, SqliteConnection};

use super::apply::IncomingThread;
use super::{SourceClockColumns, checked_sequence, source_clock_from_columns, to_sql_sequence};
use crate::error::StoreError;

/// Current canonical row positions loaded before deciding whether an observation can replace it.
///
/// Source and evidence positions are independent: a partial newer snapshot may update content
/// without proving complete parent evidence. Application compares both before selecting a write.
/// This SQL projection is private to the observation implementation, never a public read DTO.
pub struct StoredThreadObservation {
    /// Canonical thread row targeted by any accepted update.
    pub id: i64,
    /// Existing normalized discussion for same-generation payload comparison.
    pub payload_json: String,
    /// Source revision associated with current canonical content.
    pub source_clock: SourceClock,
    /// Acquisition high-water mark for that source generation.
    pub sequence: ObservationSequence,
    /// Source revision of the last accepted complete parent evidence.
    pub evidence_clock: SourceClock,
    /// Complete-evidence acquisition sequence; absent before any complete observation.
    pub evidence_sequence: Option<ObservationSequence>,
}

/// Selected canonical values bound by the thread update statement in its caller's transaction.
///
/// Application constructs this after ordering checks. Omitting `evidence_sequence` updates the
/// content high-water mark while preserving existing complete evidence. Supplying it advances
/// evidence to the same source clock as the canonical payload. This value never commits itself.
pub struct ThreadPayloadUpdate<'a> {
    /// Accepted domain snapshot supplying searchable columns and provider metadata.
    pub discussion: &'a Discussion,
    /// Serialized form of the same snapshot for later decoding and replay comparison.
    pub payload_json: &'a str,
    /// Checked SQL clock columns for the accepted source generation.
    pub source_clock: &'a SourceClockColumns,
    /// Highest acquisition sequence retained for this source generation.
    pub high_water_sequence: ObservationSequence,
    /// Local acquisition time attached to the canonical snapshot.
    pub observed_at: UtcTimestamp,
    /// New complete-evidence sequence, or no evidence change for an incomplete observation.
    pub evidence_sequence: Option<ObservationSequence>,
}

impl IncomingThread<'_> {
    /// Inserts the canonical parent row with an initially empty evidence high-water mark.
    pub async fn insert(
        &self,
        connection: &mut SqliteConnection,
        repository_row_id: i64,
    ) -> Result<i64, StoreError> {
        let discussion = self.observation.payload();
        let id: i64 = sqlx::query_scalar(
                    "INSERT INTO threads (repository_id, provider_id, number, kind, state, title, body, html_url, created_at_us, updated_at_us, closed_at_us, labels_json, assignees_json, provider_data_json, payload_json, source_clock_state, source_clock_raw, source_clock_us, observation_sequence, observed_at_us, evidence_clock_state, evidence_clock_raw, evidence_clock_us, evidence_sequence) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'missing', '', NULL, 0) RETURNING id",
                )
                .bind(repository_row_id)
                .bind(discussion.id.provider_id().as_str())
                .bind(self.number)
                .bind(thread_kind_name(discussion.kind))
                .bind(source_state_name(&discussion.state))
                .bind(&discussion.title)
                .bind(&discussion.body)
                .bind(&discussion.html_url)
                .bind(discussion.created_at.unix_microseconds())
                .bind(discussion.updated_at.unix_microseconds())
                .bind(discussion.closed_at.map(UtcTimestamp::unix_microseconds))
                .bind(serde_json::to_string(&discussion.labels)?)
                .bind(serde_json::to_string(&discussion.assignees)?)
                .bind(serde_json::to_string(&discussion.provider_data)?)
                .bind(&self.payload_json)
                .bind(self.columns.state)
                .bind(&self.columns.raw)
                .bind(self.columns.unix_microseconds)
                .bind(to_sql_sequence(self.observation.sequence())?)
                .bind(self.observation.observed_at().unix_microseconds())
                .fetch_one(&mut *connection)
                .await?;
        Ok(id)
    }
}

/// Maps a normalized discussion kind to its stored label.
fn thread_kind_name(kind: ThreadKind) -> &'static str {
    match kind {
        ThreadKind::Issue => "issue",
        ThreadKind::PullRequest => "pull_request",
    }
}

/// Maps a provider source state to its stored label.
fn source_state_name(state: &SourceState) -> &str {
    match state {
        SourceState::Open => "open",
        SourceState::Closed => "closed",
        SourceState::Other(value) => value,
    }
}

/// Loads current clocks and payload before deciding canonical replacement.
pub async fn load_thread_observation(
    connection: &mut SqliteConnection,
    repository_row_id: i64,
    provider_id: &str,
) -> Result<Option<StoredThreadObservation>, StoreError> {
    let row = sqlx::query(
        "SELECT id, payload_json, source_clock_state, source_clock_raw, source_clock_us, observation_sequence, evidence_clock_state, evidence_clock_raw, evidence_clock_us, evidence_sequence FROM threads WHERE repository_id = ? AND provider_id = ?",
    )
    .bind(repository_row_id)
    .bind(provider_id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };

    let id: i64 = row.try_get("id")?;
    let payload_json: String = row.try_get("payload_json")?;
    let source_state: String = row.try_get("source_clock_state")?;
    let source_raw: String = row.try_get("source_clock_raw")?;
    let source_microseconds: Option<i64> = row.try_get("source_clock_us")?;
    let sequence: i64 = row.try_get("observation_sequence")?;
    let evidence_state: String = row.try_get("evidence_clock_state")?;
    let evidence_raw: String = row.try_get("evidence_clock_raw")?;
    let evidence_microseconds: Option<i64> = row.try_get("evidence_clock_us")?;
    let evidence_sequence: i64 = row.try_get("evidence_sequence")?;

    Ok(Some(StoredThreadObservation {
        id,
        payload_json,
        source_clock: source_clock_from_columns(&source_state, &source_raw, source_microseconds)?,
        sequence: checked_sequence(sequence)?,
        evidence_clock: source_clock_from_columns(
            &evidence_state,
            &evidence_raw,
            evidence_microseconds,
        )?,
        evidence_sequence: if evidence_sequence == 0 {
            None
        } else {
            Some(checked_sequence(evidence_sequence)?)
        },
    }))
}

/// Updates canonical discussion content without rewriting child evidence.
pub async fn update_thread_payload(
    connection: &mut SqliteConnection,
    id: i64,
    repository_row_id: i64,
    number: i64,
    update: ThreadPayloadUpdate<'_>,
) -> Result<(), StoreError> {
    let query = if update.evidence_sequence.is_none() {
        sqlx::query(
            "UPDATE threads SET number = ?, kind = ?, state = ?, title = ?, body = ?, html_url = ?, created_at_us = ?, updated_at_us = ?, closed_at_us = ?, labels_json = ?, assignees_json = ?, provider_data_json = ?, payload_json = ?, source_clock_state = ?, source_clock_raw = ?, source_clock_us = ?, observation_sequence = ?, observed_at_us = ? WHERE id = ? AND repository_id = ?",
        )
    } else {
        sqlx::query(
            "UPDATE threads SET number = ?, kind = ?, state = ?, title = ?, body = ?, html_url = ?, created_at_us = ?, updated_at_us = ?, closed_at_us = ?, labels_json = ?, assignees_json = ?, provider_data_json = ?, payload_json = ?, source_clock_state = ?, source_clock_raw = ?, source_clock_us = ?, observation_sequence = ?, observed_at_us = ?, evidence_clock_state = ?, evidence_clock_raw = ?, evidence_clock_us = ?, evidence_sequence = ? WHERE id = ? AND repository_id = ?",
        )
    };
    let mut query = query
        .bind(number)
        .bind(thread_kind_name(update.discussion.kind))
        .bind(source_state_name(&update.discussion.state))
        .bind(&update.discussion.title)
        .bind(&update.discussion.body)
        .bind(&update.discussion.html_url)
        .bind(update.discussion.created_at.unix_microseconds())
        .bind(update.discussion.updated_at.unix_microseconds())
        .bind(
            update
                .discussion
                .closed_at
                .map(UtcTimestamp::unix_microseconds),
        )
        .bind(serde_json::to_string(&update.discussion.labels)?)
        .bind(serde_json::to_string(&update.discussion.assignees)?)
        .bind(serde_json::to_string(&update.discussion.provider_data)?)
        .bind(update.payload_json)
        .bind(update.source_clock.state)
        .bind(&update.source_clock.raw)
        .bind(update.source_clock.unix_microseconds)
        .bind(to_sql_sequence(update.high_water_sequence)?)
        .bind(update.observed_at.unix_microseconds());
    if let Some(evidence_sequence) = update.evidence_sequence {
        query = query
            .bind(update.source_clock.state)
            .bind(&update.source_clock.raw)
            .bind(update.source_clock.unix_microseconds)
            .bind(to_sql_sequence(evidence_sequence)?);
    }
    query = query.bind(id).bind(repository_row_id);
    query.execute(&mut *connection).await?;
    Ok(())
}
