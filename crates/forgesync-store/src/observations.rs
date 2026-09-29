use std::cmp::Ordering;

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::coverage::{Coverage, CoverageState, EvidenceFamily};
use forgesync_core::identity::{ObservationSequence, ProviderId, ThreadId};
use forgesync_core::observation::{CollectionCompleteness, Observation, SourceClock};
use forgesync_core::timestamp::UtcTimestamp;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqliteConnection};

use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::ordering::compare_observation_order;
use crate::{Archive, StoreError};

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

struct StoredThreadObservation {
    id: i64,
    payload_json: String,
    source_clock: SourceClock,
    sequence: ObservationSequence,
    evidence_clock: SourceClock,
    evidence_sequence: Option<ObservationSequence>,
}

struct ThreadPayloadUpdate<'a> {
    discussion: &'a Discussion,
    payload_json: &'a str,
    source_clock: &'a SourceClockColumns,
    high_water_sequence: ObservationSequence,
    observed_at: UtcTimestamp,
    evidence_sequence: Option<ObservationSequence>,
}

#[derive(Clone, Debug)]
pub(crate) struct SourceClockColumns {
    pub state: &'static str,
    pub raw: String,
    pub unix_microseconds: Option<i64>,
}

impl Archive {
    /// Reserves and durably increments the archive-wide acquisition sequence.
    pub async fn reserve_observation_sequence(
        &self,
        started_at: UtcTimestamp,
    ) -> Result<ObservationSequence, StoreError> {
        self.reserve_observation_sequence_inner(started_at, None)
            .await
    }

    /// Reserves a new observation sequence while the supplied archive lease remains current.
    pub async fn reserve_observation_sequence_fenced(
        &self,
        started_at: UtcTimestamp,
        token: &ArchiveLeaseToken,
    ) -> Result<ObservationSequence, StoreError> {
        self.reserve_observation_sequence_inner(started_at, Some(token))
            .await
    }

    async fn reserve_observation_sequence_inner(
        &self,
        started_at: UtcTimestamp,
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<ObservationSequence, StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        if let Some(token) = token {
            require_active_archive_lease(&mut transaction, token).await?;
        }
        let raw_sequence: i64 = sqlx::query_scalar(
            "UPDATE observation_sequence SET value = value + 1, last_started_at_us = ? WHERE singleton = 1 RETURNING value",
        )
        .bind(started_at.unix_microseconds())
        .fetch_one(&mut *transaction)
        .await?;
        let sequence = checked_sequence(raw_sequence)?;
        transaction.commit().await?;
        Ok(sequence)
    }

    /// Inserts or refreshes a repository identity used by discussion observations.
    pub async fn upsert_repository(&self, repository: &Repository) -> Result<i64, StoreError> {
        self.upsert_repository_inner(repository, None).await
    }

    /// Inserts or refreshes a repository only while the supplied archive lease remains current.
    pub async fn upsert_repository_fenced(
        &self,
        repository: &Repository,
        token: &ArchiveLeaseToken,
    ) -> Result<i64, StoreError> {
        self.upsert_repository_inner(repository, Some(token)).await
    }

    async fn upsert_repository_inner(
        &self,
        repository: &Repository,
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<i64, StoreError> {
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        if let Some(token) = token {
            require_active_archive_lease(&mut transaction, token).await?;
        }
        let payload_json = serde_json::to_string(repository)?;
        let provider_data_json = serde_json::to_string(&repository.provider_data)?;
        let updated_at_us = repository.updated_at.map(UtcTimestamp::unix_microseconds);
        let host = repository.id.host().as_str();
        let provider_id = repository.id.provider_id().as_str();

        let id: i64 = sqlx::query_scalar(
            "INSERT INTO repositories (host, provider_id, owner, name, full_name, default_branch, updated_at_us, provider_data_json, payload_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (host, provider_id) DO UPDATE SET owner = excluded.owner, name = excluded.name, full_name = excluded.full_name, default_branch = excluded.default_branch, updated_at_us = excluded.updated_at_us, provider_data_json = excluded.provider_data_json, payload_json = excluded.payload_json RETURNING id",
        )
        .bind(host)
        .bind(provider_id)
        .bind(&repository.owner)
        .bind(&repository.name)
        .bind(&repository.full_name)
        .bind(&repository.default_branch)
        .bind(updated_at_us)
        .bind(provider_data_json)
        .bind(payload_json)
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(id)
    }

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

    /// Reads the latest per-family completeness state; an absent row is `Missing`.
    pub async fn family_coverage(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
    ) -> Result<Coverage, StoreError> {
        let family_name = evidence_family_name(family);
        let mut connection = self.reader.acquire().await?;
        let thread_row_id = thread_row_id(&mut connection, thread).await?;
        let state_json: Option<String> = sqlx::query_scalar(
            "SELECT state_json FROM family_coverage WHERE thread_id = ? AND family = ?",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .fetch_optional(&self.reader)
        .await?;
        let state = match state_json {
            Some(json) => serde_json::from_str(&json)?,
            None => CoverageState::Missing,
        };
        Ok(Coverage::new(family, state))
    }
}

pub(crate) fn evidence_family_name(family: EvidenceFamily) -> &'static str {
    match family {
        EvidenceFamily::Threads => "threads",
        EvidenceFamily::Comments => "comments",
        EvidenceFamily::PullRequestMetadata => "pull_request_metadata",
        EvidenceFamily::Reviews => "reviews",
        EvidenceFamily::ReviewThreads => "review_threads",
    }
}

pub(crate) fn is_child_family(family: EvidenceFamily) -> bool {
    matches!(
        family,
        EvidenceFamily::Comments
            | EvidenceFamily::PullRequestMetadata
            | EvidenceFamily::Reviews
            | EvidenceFamily::ReviewThreads
    )
}

pub(crate) fn normalize_source_clock(clock: &SourceClock) -> Result<SourceClock, StoreError> {
    match clock {
        SourceClock::Invalid(raw) if raw.trim().is_empty() => Ok(SourceClock::Missing),
        SourceClock::Invalid(raw) => Ok(SourceClock::Invalid(raw.trim().to_owned())),
        SourceClock::Valid(timestamp) => Ok(SourceClock::Valid(*timestamp)),
        SourceClock::Missing => Ok(SourceClock::Missing),
    }
}

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

pub(crate) fn checked_sequence(value: i64) -> Result<ObservationSequence, StoreError> {
    if value <= 0 {
        return Err(StoreError::InvalidStoredSequence);
    }
    let value = u64::try_from(value).map_err(|_| StoreError::IntegerOutOfRange)?;
    ObservationSequence::new(value).map_err(|_| StoreError::InvalidStoredSequence)
}

pub(crate) fn to_sql_sequence(sequence: ObservationSequence) -> Result<i64, StoreError> {
    i64::try_from(sequence.get()).map_err(|_| StoreError::IntegerOutOfRange)
}

pub(crate) fn sqlite_integer(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::IntegerOutOfRange)
}

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
