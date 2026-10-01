//! Selecting and atomically applying a canonical parent (thread) observation.
//!
//! Source revision and acquisition sequence are compared independently: an older provider revision
//! never displaces newer content just because it arrived later. Content and complete-evidence
//! positions are stored separately; neither implies child-family completeness.

use std::cmp::Ordering;

use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::identity::ObservationSequence;
use forgesync_core::observation::{SourceClock, ThreadObservation};
use forgesync_core::timestamp::UtcTimestamp;
use sqlx::{Row, SqliteConnection};

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::observations::{ObservationDisposition, ThreadObservationResult};
use crate::ordering::compare_observation_order;
use crate::sql::{
    SourceClockColumns, repository_row_id, sequence_from_sql, source_clock_columns,
    source_clock_from_columns, to_sql_integer, to_sql_sequence, write_coverage,
};

impl Archive {
    /// Applies one issue or pull-request snapshot using source clock and acquisition ordering.
    ///
    /// Canonical content, evidence clocks, and coverage change in one transaction, which also
    /// checks `lease` when supplied.
    pub async fn apply_thread_observation(
        &self,
        observation: &ThreadObservation,
        lease: Option<&ArchiveLeaseToken>,
    ) -> Result<ThreadObservationResult, StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let payload_json = serde_json::to_string(&observation.discussion)?;
        let mut transaction = writer.begin().await?;
        if let Some(lease) = lease {
            require_active_archive_lease(&mut transaction, lease).await?;
        }
        let result = apply_thread(&mut transaction, observation, &payload_json).await?;
        transaction.commit().await?;
        Ok(result)
    }
}

/// Current canonical row positions; content and complete-evidence positions are independent.
struct StoredThread {
    id: i64,
    payload_json: String,
    source_clock: SourceClock,
    sequence: ObservationSequence,
    evidence_clock: SourceClock,
    /// Absent before any complete observation.
    evidence_sequence: Option<ObservationSequence>,
}

/// Selects canonical state and writes only the permitted payload, evidence, and coverage changes.
async fn apply_thread(
    connection: &mut SqliteConnection,
    observation: &ThreadObservation,
    payload_json: &str,
) -> Result<ThreadObservationResult, StoreError> {
    let discussion = &observation.discussion;
    let sequence = observation.sequence;
    let clock = SourceClock::Valid(discussion.updated_at);
    let columns = source_clock_columns(&clock);
    let repository = repository_row_id(connection, discussion.id.repository()).await?;

    let (id, canonical_updated, high_water, evidence_clock, stored_evidence_sequence) =
        match load_thread(connection, repository, discussion.id.provider_id().as_str()).await? {
            None => {
                let id = insert_thread(connection, repository, observation, payload_json, &columns)
                    .await?;
                (id, true, sequence, clock.clone(), None)
            }
            Some(existing) => {
                let skipped = ThreadObservationResult {
                    thread_row_id: existing.id,
                    disposition: ObservationDisposition::Skipped,
                    high_water_sequence: existing.sequence,
                    evidence_sequence: existing.evidence_sequence,
                };
                // Comparing with equal sequences isolates the source-revision order.
                let source_order =
                    compare_observation_order(&clock, sequence, &existing.source_clock, sequence)?;
                if source_order == Ordering::Less {
                    return Ok(skipped);
                }
                let same_payload = payload_json == existing.payload_json;
                if source_order == Ordering::Equal && sequence == existing.sequence && !same_payload
                {
                    return Err(StoreError::ConflictingObservation);
                }
                let order = compare_observation_order(
                    &clock,
                    sequence,
                    &existing.source_clock,
                    existing.sequence,
                )?;
                // Older archives may hold incomplete parent evidence below the content high-water
                // mark; an identical complete payload can still hydrate that evidence.
                let hydrate =
                    source_order == Ordering::Equal && sequence < existing.sequence && same_payload;
                if order == Ordering::Less && !hydrate {
                    return Ok(skipped);
                }
                let canonical_updated = order == Ordering::Greater;
                let high_water = if canonical_updated {
                    sequence
                } else {
                    existing.sequence
                };
                (
                    existing.id,
                    canonical_updated,
                    high_water,
                    existing.evidence_clock,
                    existing.evidence_sequence,
                )
            }
        };

    // Only complete evidence newer than the current complete-evidence position advances it.
    let accepts_evidence = match stored_evidence_sequence {
        None => true,
        Some(current) => {
            compare_observation_order(&clock, sequence, &evidence_clock, current)?
                == Ordering::Greater
        }
    };
    let evidence_sequence = accepts_evidence.then_some(sequence);
    if canonical_updated {
        update_thread(
            connection,
            id,
            repository,
            observation,
            payload_json,
            &columns,
            high_water,
            evidence_sequence,
        )
        .await?;
    } else if let Some(evidence_sequence) = evidence_sequence {
        sqlx::query(
            "UPDATE threads SET evidence_clock_state = ?, evidence_clock_raw = ?, evidence_clock_us = ?, evidence_sequence = ? WHERE id = ?",
        )
        .bind(columns.state)
        .bind(&columns.raw)
        .bind(columns.unix_microseconds)
        .bind(to_sql_sequence(evidence_sequence)?)
        .bind(id)
        .execute(&mut *connection)
        .await?;
    }

    let disposition = if canonical_updated || evidence_sequence.is_some() {
        let state = CoverageState::Complete {
            observed_at: observation.observed_at,
            sequence,
            item_count: 1,
        };
        write_coverage(connection, id, EvidenceFamily::Threads, &columns, &state).await?;
        ObservationDisposition::Applied
    } else {
        ObservationDisposition::Replayed
    };
    Ok(ThreadObservationResult {
        thread_row_id: id,
        disposition,
        high_water_sequence: high_water,
        evidence_sequence: evidence_sequence.or(stored_evidence_sequence),
    })
}

/// Loads current clocks and payload before deciding canonical replacement.
async fn load_thread(
    connection: &mut SqliteConnection,
    repository_row_id: i64,
    provider_id: &str,
) -> Result<Option<StoredThread>, StoreError> {
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
    let evidence_sequence: i64 = row.try_get("evidence_sequence")?;
    Ok(Some(StoredThread {
        id: row.try_get("id")?,
        payload_json: row.try_get("payload_json")?,
        source_clock: source_clock_from_columns(
            &row.try_get::<String, _>("source_clock_state")?,
            &row.try_get::<String, _>("source_clock_raw")?,
            row.try_get("source_clock_us")?,
        )?,
        sequence: sequence_from_sql(row.try_get("observation_sequence")?)?,
        evidence_clock: source_clock_from_columns(
            &row.try_get::<String, _>("evidence_clock_state")?,
            &row.try_get::<String, _>("evidence_clock_raw")?,
            row.try_get("evidence_clock_us")?,
        )?,
        // Zero is the stored "no complete evidence yet" marker.
        evidence_sequence: if evidence_sequence == 0 {
            None
        } else {
            Some(sequence_from_sql(evidence_sequence)?)
        },
    }))
}

/// Inserts the canonical parent row with an initially empty evidence high-water mark.
async fn insert_thread(
    connection: &mut SqliteConnection,
    repository_row_id: i64,
    observation: &ThreadObservation,
    payload_json: &str,
    columns: &SourceClockColumns,
) -> Result<i64, StoreError> {
    let discussion = &observation.discussion;
    Ok(sqlx::query_scalar(
        "INSERT INTO threads (repository_id, provider_id, number, kind, state, title, body, html_url, created_at_us, updated_at_us, closed_at_us, labels_json, assignees_json, provider_data_json, payload_json, source_clock_state, source_clock_raw, source_clock_us, observation_sequence, observed_at_us, evidence_clock_state, evidence_clock_raw, evidence_clock_us, evidence_sequence) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'missing', '', NULL, 0) RETURNING id",
    )
    .bind(repository_row_id)
    .bind(discussion.id.provider_id().as_str())
    .bind(to_sql_integer(discussion.id.number().get())?)
    .bind(discussion.kind.as_str())
    .bind(discussion.state.as_str())
    .bind(&discussion.title)
    .bind(&discussion.body)
    .bind(&discussion.html_url)
    .bind(discussion.created_at.unix_microseconds())
    .bind(discussion.updated_at.unix_microseconds())
    .bind(discussion.closed_at.map(UtcTimestamp::unix_microseconds))
    .bind(serde_json::to_string(&discussion.labels)?)
    .bind(serde_json::to_string(&discussion.assignees)?)
    .bind(serde_json::to_string(&discussion.provider_data)?)
    .bind(payload_json)
    .bind(columns.state)
    .bind(&columns.raw)
    .bind(columns.unix_microseconds)
    .bind(to_sql_sequence(observation.sequence)?)
    .bind(observation.observed_at.unix_microseconds())
    .fetch_one(&mut *connection)
    .await?)
}

/// Replaces canonical content; evidence columns move only when `evidence_sequence` is set.
#[allow(
    clippy::too_many_arguments,
    reason = "one bound UPDATE statement; grouping the values would only rename them"
)]
async fn update_thread(
    connection: &mut SqliteConnection,
    id: i64,
    repository_row_id: i64,
    observation: &ThreadObservation,
    payload_json: &str,
    columns: &SourceClockColumns,
    high_water_sequence: ObservationSequence,
    evidence_sequence: Option<ObservationSequence>,
) -> Result<(), StoreError> {
    let discussion = &observation.discussion;
    let query = if evidence_sequence.is_none() {
        sqlx::query(
            "UPDATE threads SET number = ?, kind = ?, state = ?, title = ?, body = ?, html_url = ?, created_at_us = ?, updated_at_us = ?, closed_at_us = ?, labels_json = ?, assignees_json = ?, provider_data_json = ?, payload_json = ?, source_clock_state = ?, source_clock_raw = ?, source_clock_us = ?, observation_sequence = ?, observed_at_us = ? WHERE id = ? AND repository_id = ?",
        )
    } else {
        sqlx::query(
            "UPDATE threads SET number = ?, kind = ?, state = ?, title = ?, body = ?, html_url = ?, created_at_us = ?, updated_at_us = ?, closed_at_us = ?, labels_json = ?, assignees_json = ?, provider_data_json = ?, payload_json = ?, source_clock_state = ?, source_clock_raw = ?, source_clock_us = ?, observation_sequence = ?, observed_at_us = ?, evidence_clock_state = ?, evidence_clock_raw = ?, evidence_clock_us = ?, evidence_sequence = ? WHERE id = ? AND repository_id = ?",
        )
    };
    let mut query = query
        .bind(to_sql_integer(discussion.id.number().get())?)
        .bind(discussion.kind.as_str())
        .bind(discussion.state.as_str())
        .bind(&discussion.title)
        .bind(&discussion.body)
        .bind(&discussion.html_url)
        .bind(discussion.created_at.unix_microseconds())
        .bind(discussion.updated_at.unix_microseconds())
        .bind(discussion.closed_at.map(UtcTimestamp::unix_microseconds))
        .bind(serde_json::to_string(&discussion.labels)?)
        .bind(serde_json::to_string(&discussion.assignees)?)
        .bind(serde_json::to_string(&discussion.provider_data)?)
        .bind(payload_json)
        .bind(columns.state)
        .bind(&columns.raw)
        .bind(columns.unix_microseconds)
        .bind(to_sql_sequence(high_water_sequence)?)
        .bind(observation.observed_at.unix_microseconds());
    if let Some(evidence_sequence) = evidence_sequence {
        query = query
            .bind(columns.state)
            .bind(&columns.raw)
            .bind(columns.unix_microseconds)
            .bind(to_sql_sequence(evidence_sequence)?);
    }
    query
        .bind(id)
        .bind(repository_row_id)
        .execute(&mut *connection)
        .await?;
    Ok(())
}
