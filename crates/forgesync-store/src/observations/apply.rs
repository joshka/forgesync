//! Observation apply operations.

use sqlx::Row;

use super::{
    Archive, ArchiveLeaseToken, CollectionCompleteness, CoverageState, Discussion, EvidenceFamily,
    Observation, ObservationDisposition, ObservationSequence, Ordering, SourceClock, SourceState,
    SqliteConnection, StoreError, StoredThreadObservation, ThreadKind, ThreadObservationResult,
    ThreadPayloadUpdate, UtcTimestamp, checked_sequence, compare_observation_order,
    normalize_source_clock, repository_row_id, require_active_archive_lease, source_clock_columns,
    source_clock_from_columns, sqlite_integer, to_sql_sequence, write_coverage,
};

impl Archive {
    /// Applies one issue or pull-request snapshot using source clock and acquisition ordering.
    pub async fn apply_thread_observation(
        &self,
        observation: &Observation<Discussion>,
    ) -> Result<ThreadObservationResult, StoreError> {
        self.apply_thread_observation_inner(observation, None).await
    }

    /// Applies a thread snapshot only while the supplied archive lease remains current.
    pub async fn apply_thread_observation_fenced(
        &self,
        observation: &Observation<Discussion>,
        token: &ArchiveLeaseToken,
    ) -> Result<ThreadObservationResult, StoreError> {
        self.apply_thread_observation_inner(observation, Some(token))
            .await
    }

    async fn apply_thread_observation_inner(
        &self,
        observation: &Observation<Discussion>,
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<ThreadObservationResult, StoreError> {
        if observation.family() != EvidenceFamily::Threads {
            return Err(StoreError::ObservationFamilyMismatch);
        }

        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let discussion = observation.payload();
        let payload_json = serde_json::to_string(discussion)?;
        let incoming_clock = normalize_source_clock(observation.source_clock())?;
        let incoming_clock_columns = source_clock_columns(&incoming_clock)?;
        let thread_number = sqlite_integer(discussion.id.number().get())?;
        let incoming_sequence = observation.sequence();
        let completeness = observation.completeness();

        let mut transaction = writer.begin().await?;
        if let Some(token) = token {
            require_active_archive_lease(&mut transaction, token).await?;
        }
        let repository_row_id = repository_row_id(
            &mut transaction,
            discussion.id.repository().host().as_str(),
            discussion.id.repository().provider_id().as_str(),
        )
        .await?;
        let existing = load_thread_observation(
            &mut transaction,
            repository_row_id,
            discussion.id.provider_id().as_str(),
        )
        .await?;

        let (
            thread_row_id,
            canonical_updated,
            high_water,
            current_evidence_clock,
            current_evidence_sequence,
        ) = if let Some(existing) = existing {
            let source_order = compare_observation_order(
                &incoming_clock,
                incoming_sequence,
                &existing.source_clock,
                incoming_sequence,
            )?;
            if source_order == Ordering::Less {
                transaction.commit().await?;
                return Ok(ThreadObservationResult {
                    thread_row_id: existing.id,
                    disposition: ObservationDisposition::Skipped,
                    high_water_sequence: existing.sequence,
                    evidence_sequence: existing.evidence_sequence,
                });
            }

            let same_payload = payload_json == existing.payload_json;
            let sequence_order = compare_observation_order(
                &incoming_clock,
                incoming_sequence,
                &existing.source_clock,
                existing.sequence,
            )?;
            if source_order == Ordering::Equal
                && incoming_sequence == existing.sequence
                && !same_payload
            {
                return Err(StoreError::ConflictingObservation);
            }

            let hydrate_same_payload = source_order == Ordering::Equal
                && incoming_sequence < existing.sequence
                && same_payload
                && matches!(completeness, CollectionCompleteness::Complete);
            if sequence_order == Ordering::Less && !hydrate_same_payload {
                transaction.commit().await?;
                return Ok(ThreadObservationResult {
                    thread_row_id: existing.id,
                    disposition: ObservationDisposition::Skipped,
                    high_water_sequence: existing.sequence,
                    evidence_sequence: existing.evidence_sequence,
                });
            }

            let canonical_updated = sequence_order == Ordering::Greater;
            let high_water = if canonical_updated {
                incoming_sequence
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
        } else {
            let id: i64 = sqlx::query_scalar(
                    "INSERT INTO threads (repository_id, provider_id, number, kind, state, title, body, html_url, created_at_us, updated_at_us, closed_at_us, labels_json, assignees_json, provider_data_json, payload_json, source_clock_state, source_clock_raw, source_clock_us, observation_sequence, observed_at_us, evidence_clock_state, evidence_clock_raw, evidence_clock_us, evidence_sequence) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'missing', '', NULL, 0) RETURNING id",
                )
                .bind(repository_row_id)
                .bind(discussion.id.provider_id().as_str())
                .bind(thread_number)
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
                .bind(&payload_json)
                .bind(incoming_clock_columns.state)
                .bind(&incoming_clock_columns.raw)
                .bind(incoming_clock_columns.unix_microseconds)
                .bind(to_sql_sequence(incoming_sequence)?)
                .bind(observation.observed_at().unix_microseconds())
                .fetch_one(&mut *transaction)
                .await?;
            let evidence_sequence = if matches!(completeness, CollectionCompleteness::Complete) {
                Some(incoming_sequence)
            } else {
                None
            };
            let evidence_clock = if evidence_sequence.is_some() {
                incoming_clock.clone()
            } else {
                SourceClock::Missing
            };
            (id, true, incoming_sequence, evidence_clock, None)
        };

        let evidence_applied = if matches!(completeness, CollectionCompleteness::Complete) {
            match current_evidence_sequence {
                None => true,
                Some(current_sequence) => {
                    compare_observation_order(
                        &incoming_clock,
                        incoming_sequence,
                        &current_evidence_clock,
                        current_sequence,
                    )? == Ordering::Greater
                }
            }
        } else {
            false
        };

        if canonical_updated {
            update_thread_payload(
                &mut transaction,
                thread_row_id,
                repository_row_id,
                thread_number,
                ThreadPayloadUpdate {
                    discussion,
                    payload_json: &payload_json,
                    source_clock: &incoming_clock_columns,
                    high_water_sequence: high_water,
                    observed_at: observation.observed_at(),
                    evidence_sequence: evidence_applied.then_some(incoming_sequence),
                },
            )
            .await?;
        } else if evidence_applied {
            sqlx::query(
                "UPDATE threads SET evidence_clock_state = ?, evidence_clock_raw = ?, evidence_clock_us = ?, evidence_sequence = ? WHERE id = ?",
            )
            .bind(incoming_clock_columns.state)
            .bind(&incoming_clock_columns.raw)
            .bind(incoming_clock_columns.unix_microseconds)
            .bind(to_sql_sequence(incoming_sequence)?)
            .bind(thread_row_id)
            .execute(&mut *transaction)
            .await?;
        }

        let disposition = if canonical_updated || evidence_applied {
            let state = coverage_state(
                completeness,
                observation.observed_at(),
                incoming_sequence,
                1,
            );
            write_coverage(
                &mut transaction,
                thread_row_id,
                EvidenceFamily::Threads,
                &incoming_clock_columns,
                observation.observed_at(),
                incoming_sequence,
                &state,
            )
            .await?;
            ObservationDisposition::Applied
        } else {
            ObservationDisposition::Replayed
        };

        transaction.commit().await?;
        Ok(ThreadObservationResult {
            thread_row_id,
            disposition,
            high_water_sequence: high_water,
            evidence_sequence: if evidence_applied {
                Some(incoming_sequence)
            } else {
                current_evidence_sequence
            },
        })
    }
}

fn thread_kind_name(kind: ThreadKind) -> &'static str {
    match kind {
        ThreadKind::Issue => "issue",
        ThreadKind::PullRequest => "pull_request",
    }
}

fn source_state_name(state: &SourceState) -> &str {
    match state {
        SourceState::Open => "open",
        SourceState::Closed => "closed",
        SourceState::Other(value) => value,
    }
}

fn coverage_state(
    completeness: &CollectionCompleteness,
    observed_at: UtcTimestamp,
    sequence: ObservationSequence,
    item_count: u64,
) -> CoverageState {
    match completeness {
        CollectionCompleteness::Complete => CoverageState::Complete {
            observed_at,
            sequence,
            item_count,
        },
        CollectionCompleteness::Incomplete {
            reason,
            received_items,
        } => CoverageState::Incomplete {
            observed_at,
            sequence,
            reason: *reason,
            received_items: *received_items,
            failure: None,
        },
    }
}

async fn load_thread_observation(
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

async fn update_thread_payload(
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
