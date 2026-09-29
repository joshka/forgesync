use std::collections::BTreeMap;

use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::identity::{CommitSha, ObservationSequence, ThreadId};
use forgesync_core::observation::{CollectionCompleteness, SourceClock};
use forgesync_core::timestamp::UtcTimestamp;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::{Row, SqliteConnection};

use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::observations::{
    FamilyObservationResult, FamilyReservation, ObservationDisposition, StagedItem,
    checked_sequence, evidence_family_name, is_child_family, normalize_source_clock,
    source_clock_columns, source_clock_from_columns, thread_row_id, to_sql_sequence,
    write_coverage,
};
use crate::ordering::compare_observation_order;
use crate::{Archive, StoreError};

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

impl Archive {
    /// Allocates a sequence and tries to reserve an independently ordered child family.
    pub async fn reserve_child_family_observation(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        source_clock: &SourceClock,
        started_at: UtcTimestamp,
        request_scope: &str,
    ) -> Result<FamilyReservation, StoreError> {
        self.reserve_child_family_observation_inner(
            thread,
            family,
            source_clock,
            started_at,
            request_scope,
            None,
        )
        .await
    }

    /// Reserves a child family only while the supplied archive lease remains current.
    pub async fn reserve_child_family_observation_fenced(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        source_clock: &SourceClock,
        started_at: UtcTimestamp,
        request_scope: &str,
        token: &ArchiveLeaseToken,
    ) -> Result<FamilyReservation, StoreError> {
        self.reserve_child_family_observation_inner(
            thread,
            family,
            source_clock,
            started_at,
            request_scope,
            Some(token),
        )
        .await
    }

    async fn reserve_child_family_observation_inner(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        source_clock: &SourceClock,
        started_at: UtcTimestamp,
        request_scope: &str,
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<FamilyReservation, StoreError> {
        if !is_child_family(family) {
            return Err(StoreError::UnsupportedObservationFamily(
                evidence_family_name(family).to_owned(),
            ));
        }
        let request_scope = request_scope.trim();
        if request_scope.is_empty() {
            return Err(StoreError::MissingRequestScope);
        }

        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let source_clock = normalize_source_clock(source_clock)?;
        let source_clock_fields = source_clock_columns(&source_clock)?;
        let family_name = evidence_family_name(family);
        let mut transaction = writer.begin().await?;
        if let Some(token) = token {
            require_active_archive_lease(&mut transaction, token).await?;
        }
        let thread_row_id = thread_row_id(&mut transaction, thread).await?;

        let raw_sequence: i64 = sqlx::query_scalar(
            "UPDATE observation_sequence SET value = value + 1, last_started_at_us = ? WHERE singleton = 1 RETURNING value",
        )
        .bind(started_at.unix_microseconds())
        .fetch_one(&mut *transaction)
        .await?;
        let sequence = checked_sequence(raw_sequence)?;

        let current = sqlx::query(
            "SELECT source_clock_state, source_clock_raw, source_clock_us, sequence FROM thread_family_reservations WHERE thread_id = ? AND family = ?",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(row) = current {
            let state: String = row.try_get("source_clock_state")?;
            let raw: String = row.try_get("source_clock_raw")?;
            let microseconds: Option<i64> = row.try_get("source_clock_us")?;
            let current_sequence: i64 = row.try_get("sequence")?;
            let current_clock = source_clock_from_columns(&state, &raw, microseconds)?;
            let order = compare_observation_order(
                &source_clock,
                sequence,
                &current_clock,
                checked_sequence(current_sequence)?,
            )?;
            if order != std::cmp::Ordering::Greater {
                transaction.commit().await?;
                return Ok(FamilyReservation {
                    sequence,
                    reserved: false,
                });
            }
        }

        sqlx::query(
            "INSERT INTO thread_family_reservations (thread_id, family, source_clock_state, source_clock_raw, source_clock_us, sequence, started_at_us, request_scope) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (thread_id, family) DO UPDATE SET source_clock_state = excluded.source_clock_state, source_clock_raw = excluded.source_clock_raw, source_clock_us = excluded.source_clock_us, sequence = excluded.sequence, started_at_us = excluded.started_at_us, request_scope = excluded.request_scope",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .bind(source_clock_fields.state)
        .bind(&source_clock_fields.raw)
        .bind(source_clock_fields.unix_microseconds)
        .bind(to_sql_sequence(sequence)?)
        .bind(started_at.unix_microseconds())
        .bind(request_scope)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO observation_generations (thread_id, family, sequence, source_clock_state, source_clock_raw, source_clock_us, started_at_us, request_scope, status) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'reserved')",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .bind(to_sql_sequence(sequence)?)
        .bind(source_clock_fields.state)
        .bind(&source_clock_fields.raw)
        .bind(source_clock_fields.unix_microseconds)
        .bind(started_at.unix_microseconds())
        .bind(request_scope)
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
        Ok(FamilyReservation {
            sequence,
            reserved: true,
        })
    }

    /// Persists one page for a reserved generation. Replaying the same page is idempotent.
    pub async fn stage_child_family_page<T>(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        sequence: ObservationSequence,
        page_index: u32,
        items: &[StagedItem<T>],
    ) -> Result<(), StoreError>
    where
        T: Serialize,
    {
        self.stage_child_family_page_inner(thread, family, sequence, page_index, items, None)
            .await
    }

    /// Stages a child-family page only while the supplied archive lease remains current.
    pub async fn stage_child_family_page_fenced<T>(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        sequence: ObservationSequence,
        page_index: u32,
        items: &[StagedItem<T>],
        token: &ArchiveLeaseToken,
    ) -> Result<(), StoreError>
    where
        T: Serialize,
    {
        self.stage_child_family_page_inner(thread, family, sequence, page_index, items, Some(token))
            .await
    }

    async fn stage_child_family_page_inner<T>(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        sequence: ObservationSequence,
        page_index: u32,
        items: &[StagedItem<T>],
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<(), StoreError>
    where
        T: Serialize,
    {
        if !is_child_family(family) {
            return Err(StoreError::UnsupportedObservationFamily(
                evidence_family_name(family).to_owned(),
            ));
        }
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let family_name = evidence_family_name(family);
        let page_index = i64::from(page_index);
        let payload_json = serde_json::to_string(items)?;
        let mut transaction = writer.begin().await?;
        if let Some(token) = token {
            require_active_archive_lease(&mut transaction, token).await?;
        }
        let thread_row_id = thread_row_id(&mut transaction, thread).await?;
        let current_sequence: Option<i64> = sqlx::query_scalar(
            "SELECT sequence FROM thread_family_reservations WHERE thread_id = ? AND family = ?",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .fetch_optional(&mut *transaction)
        .await?;
        if current_sequence != Some(to_sql_sequence(sequence)?) {
            return Err(StoreError::StaleObservationGeneration);
        }

        let generation_status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM observation_generations WHERE thread_id = ? AND family = ? AND sequence = ?",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .bind(to_sql_sequence(sequence)?)
        .fetch_optional(&mut *transaction)
        .await?;
        match generation_status.as_deref() {
            None => return Err(StoreError::ObservationGenerationMissing),
            Some("complete") => return Err(StoreError::StaleObservationGeneration),
            Some("reserved" | "incomplete") => {}
            Some(_) => return Err(StoreError::ObservationGenerationMissing),
        }

        let existing_page: Option<String> = sqlx::query_scalar(
            "SELECT payload_json FROM observation_staging_pages WHERE thread_id = ? AND family = ? AND sequence = ? AND page_index = ?",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .bind(to_sql_sequence(sequence)?)
        .bind(page_index)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(existing_page) = existing_page {
            if existing_page != payload_json {
                return Err(StoreError::StagedPageConflict);
            }
            transaction.commit().await?;
            return Ok(());
        }

        sqlx::query(
            "INSERT INTO observation_staging_pages (thread_id, family, sequence, page_index, payload_json) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .bind(to_sql_sequence(sequence)?)
        .bind(page_index)
        .bind(payload_json)
        .execute(&mut *transaction)
        .await?;
        let staged_pages = load_staged_pages(
            &mut transaction,
            thread_row_id,
            family_name,
            to_sql_sequence(sequence)?,
        )
        .await?;
        let staged_count = count_staged_items(&staged_pages)?;
        sqlx::query(
            "UPDATE observation_generations SET status = 'reserved', received_items = ? WHERE thread_id = ? AND family = ? AND sequence = ?",
        )
        .bind(i64::try_from(staged_count).map_err(|_| StoreError::IntegerOutOfRange)?)
        .bind(thread_row_id)
        .bind(family_name)
        .bind(to_sql_sequence(sequence)?)
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
        Ok(())
    }

    /// Commits a complete membership snapshot or records an incomplete attempt without replacing
    /// it.
    pub async fn finish_child_family_observation(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        sequence: ObservationSequence,
        observed_at: UtcTimestamp,
        completeness: &CollectionCompleteness,
        expected_pages: Option<u32>,
    ) -> Result<FamilyObservationResult, StoreError> {
        let observation = ChildFamilyObservation {
            thread,
            family,
            sequence,
            observed_at,
            completeness,
            expected_pages,
            head_sha: None,
        };
        self.finish_child_family_observation_with_context(observation)
            .await
    }

    /// Finalizes a child family with its acquisition context and without an archive lease.
    pub async fn finish_child_family_observation_with_context(
        &self,
        observation: ChildFamilyObservation<'_>,
    ) -> Result<FamilyObservationResult, StoreError> {
        self.finish_child_family_observation_inner(observation, None)
            .await
    }

    /// Finalizes a child family only while the supplied archive lease remains current.
    pub async fn finish_child_family_observation_fenced(
        &self,
        observation: ChildFamilyObservation<'_>,
        token: &ArchiveLeaseToken,
    ) -> Result<FamilyObservationResult, StoreError> {
        self.finish_child_family_observation_inner(observation, Some(token))
            .await
    }

    async fn finish_child_family_observation_inner(
        &self,
        observation: ChildFamilyObservation<'_>,
        token: Option<&ArchiveLeaseToken>,
    ) -> Result<FamilyObservationResult, StoreError> {
        let ChildFamilyObservation {
            thread,
            family,
            sequence,
            observed_at,
            completeness,
            expected_pages,
            head_sha,
        } = observation;
        if !is_child_family(family) {
            return Err(StoreError::UnsupportedObservationFamily(
                evidence_family_name(family).to_owned(),
            ));
        }
        match completeness {
            CollectionCompleteness::Complete if expected_pages.is_none() => {
                return Err(StoreError::MissingExpectedPageCount);
            }
            CollectionCompleteness::Incomplete { .. } if expected_pages.is_some() => {
                return Err(StoreError::InvalidCollectionCompleteness);
            }
            _ => {}
        }
        let head_bound_family = matches!(
            family,
            EvidenceFamily::Reviews | EvidenceFamily::ReviewThreads
        );
        if head_sha.is_some() && !head_bound_family {
            return Err(StoreError::UnexpectedPullRequestHeadContext);
        }
        if head_bound_family
            && matches!(completeness, CollectionCompleteness::Complete)
            && head_sha.is_none()
        {
            return Err(StoreError::MissingPullRequestHeadContext);
        }

        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let family_name = evidence_family_name(family);
        let sequence_value = to_sql_sequence(sequence)?;
        let mut transaction = writer.begin().await?;
        if let Some(token) = token {
            require_active_archive_lease(&mut transaction, token).await?;
        }
        let thread_row_id = thread_row_id(&mut transaction, thread).await?;
        let current_sequence: Option<i64> = sqlx::query_scalar(
            "SELECT sequence FROM thread_family_reservations WHERE thread_id = ? AND family = ?",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .fetch_optional(&mut *transaction)
        .await?;
        if current_sequence != Some(sequence_value) {
            transaction.commit().await?;
            return Ok(FamilyObservationResult {
                disposition: ObservationDisposition::Skipped,
                item_count: 0,
            });
        }

        let generation = sqlx::query(
            "SELECT source_clock_state, source_clock_raw, source_clock_us, status, item_count FROM observation_generations WHERE thread_id = ? AND family = ? AND sequence = ?",
        )
        .bind(thread_row_id)
        .bind(family_name)
        .bind(sequence_value)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(StoreError::ObservationGenerationMissing)?;
        let source_state: String = generation.try_get("source_clock_state")?;
        let source_raw: String = generation.try_get("source_clock_raw")?;
        let source_microseconds: Option<i64> = generation.try_get("source_clock_us")?;
        let generation_status: String = generation.try_get("status")?;
        let prior_item_count: i64 = generation.try_get("item_count")?;
        if generation_status == "complete" {
            transaction.commit().await?;
            return Ok(FamilyObservationResult {
                disposition: ObservationDisposition::Replayed,
                item_count: u64::try_from(prior_item_count)
                    .map_err(|_| StoreError::InvalidStoredSequence)?,
            });
        }
        let source_clock =
            source_clock_from_columns(&source_state, &source_raw, source_microseconds)?;
        let source_clock_fields = source_clock_columns(&source_clock)?;
        let pages =
            load_staged_pages(&mut transaction, thread_row_id, family_name, sequence_value).await?;
        let staged_count = count_staged_items(&pages)?;

        match completeness {
            CollectionCompleteness::Incomplete {
                reason,
                received_items,
            } => {
                if *received_items != staged_count {
                    return Err(StoreError::InvalidCollectionCompleteness);
                }
                let state = CoverageState::Incomplete {
                    observed_at,
                    sequence,
                    reason: *reason,
                    received_items: *received_items,
                    failure: None,
                };
                write_coverage(
                    &mut transaction,
                    thread_row_id,
                    family,
                    &source_clock_fields,
                    observed_at,
                    sequence,
                    &state,
                )
                .await?;
                sqlx::query(
                    "UPDATE observation_generations SET status = 'incomplete', received_items = ?, item_count = ? WHERE thread_id = ? AND family = ? AND sequence = ?",
                )
                .bind(i64::try_from(*received_items).map_err(|_| StoreError::IntegerOutOfRange)?)
                .bind(i64::try_from(staged_count).map_err(|_| StoreError::IntegerOutOfRange)?)
                .bind(thread_row_id)
                .bind(family_name)
                .bind(sequence_value)
                .execute(&mut *transaction)
                .await?;
                transaction.commit().await?;
                Ok(FamilyObservationResult {
                    disposition: ObservationDisposition::Applied,
                    item_count: staged_count,
                })
            }
            CollectionCompleteness::Complete => {
                let expected_pages = expected_pages.ok_or(StoreError::MissingExpectedPageCount)?;
                validate_page_set(&pages, expected_pages)?;
                let items = merge_staged_items(&pages)?;
                sqlx::query(
                    "DELETE FROM thread_family_membership WHERE thread_id = ? AND family = ?",
                )
                .bind(thread_row_id)
                .bind(family_name)
                .execute(&mut *transaction)
                .await?;

                for item in items.values() {
                    let item_payload = serde_json::to_string(&item.payload)?;
                    sqlx::query(
                        "INSERT INTO thread_family_membership (thread_id, family, provider_id, payload_json, sequence) VALUES (?, ?, ?, ?, ?)",
                    )
                    .bind(thread_row_id)
                    .bind(family_name)
                    .bind(item.id.as_str())
                    .bind(item_payload)
                    .bind(sequence_value)
                    .execute(&mut *transaction)
                    .await?;
                }

                let item_count =
                    u64::try_from(items.len()).map_err(|_| StoreError::IntegerOutOfRange)?;
                let state = CoverageState::Complete {
                    observed_at,
                    sequence,
                    item_count,
                };
                write_coverage(
                    &mut transaction,
                    thread_row_id,
                    family,
                    &source_clock_fields,
                    observed_at,
                    sequence,
                    &state,
                )
                .await?;
                if let Some(head_sha) = head_sha {
                    sqlx::query(
                        "INSERT INTO thread_family_head_contexts (thread_id, family, head_sha, sequence) VALUES (?, ?, ?, ?) ON CONFLICT (thread_id, family) DO UPDATE SET head_sha = excluded.head_sha, sequence = excluded.sequence",
                    )
                    .bind(thread_row_id)
                    .bind(family_name)
                    .bind(head_sha.as_str())
                    .bind(sequence_value)
                    .execute(&mut *transaction)
                    .await?;
                }
                sqlx::query(
                    "UPDATE observation_generations SET status = 'complete', received_items = ?, item_count = ? WHERE thread_id = ? AND family = ? AND sequence = ?",
                )
                .bind(i64::try_from(staged_count).map_err(|_| StoreError::IntegerOutOfRange)?)
                .bind(i64::try_from(item_count).map_err(|_| StoreError::IntegerOutOfRange)?)
                .bind(thread_row_id)
                .bind(family_name)
                .bind(sequence_value)
                .execute(&mut *transaction)
                .await?;
                sqlx::query(
                    "DELETE FROM observation_staging_pages WHERE thread_id = ? AND family = ?",
                )
                .bind(thread_row_id)
                .bind(family_name)
                .execute(&mut *transaction)
                .await?;
                sqlx::query(
                    "DELETE FROM observation_generations WHERE thread_id = ? AND family = ? AND sequence <> ?",
                )
                .bind(thread_row_id)
                .bind(family_name)
                .bind(sequence_value)
                .execute(&mut *transaction)
                .await?;

                transaction.commit().await?;
                Ok(FamilyObservationResult {
                    disposition: ObservationDisposition::Applied,
                    item_count,
                })
            }
        }
    }

    /// Returns the canonical complete membership for one thread family.
    pub async fn child_family_members<T>(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
    ) -> Result<Vec<StagedItem<T>>, StoreError>
    where
        T: DeserializeOwned,
    {
        if !is_child_family(family) {
            return Err(StoreError::UnsupportedObservationFamily(
                evidence_family_name(family).to_owned(),
            ));
        }
        let mut connection = self.reader.acquire().await?;
        let thread_row_id = thread_row_id(&mut connection, thread).await?;
        let rows = sqlx::query(
            "SELECT provider_id, payload_json FROM thread_family_membership WHERE thread_id = ? AND family = ? ORDER BY provider_id",
        )
        .bind(thread_row_id)
        .bind(evidence_family_name(family))
        .fetch_all(&mut *connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                let id: String = row.try_get("provider_id")?;
                let payload_json: String = row.try_get("payload_json")?;
                Ok(StagedItem {
                    id: serde_json::from_value(serde_json::Value::String(id))?,
                    payload: serde_json::from_str(&payload_json)?,
                })
            })
            .collect()
    }

    /// Returns whether the latest complete family snapshot matches the current parent clock and
    /// expected member count. An unknown count deliberately forces a refresh.
    pub async fn child_family_is_current(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        source_clock: &SourceClock,
        expected_item_count: Option<u64>,
    ) -> Result<bool, StoreError> {
        self.child_family_is_current_inner(
            thread,
            family,
            source_clock,
            expected_item_count,
            None,
            false,
        )
        .await
    }

    /// Returns whether head-bound pull-request evidence is complete for the same source clock and
    /// head. The stored count is checked against canonical membership because the parent issue row
    /// does not expose these family counts.
    pub async fn pull_request_family_is_current_for_head(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        source_clock: &SourceClock,
        head_sha: &CommitSha,
    ) -> Result<bool, StoreError> {
        if !matches!(
            family,
            EvidenceFamily::Reviews | EvidenceFamily::ReviewThreads
        ) {
            return Err(StoreError::UnexpectedPullRequestHeadContext);
        }
        self.child_family_is_current_inner(thread, family, source_clock, None, Some(head_sha), true)
            .await
    }

    async fn child_family_is_current_inner(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        source_clock: &SourceClock,
        expected_item_count: Option<u64>,
        expected_head_sha: Option<&CommitSha>,
        allow_stored_count: bool,
    ) -> Result<bool, StoreError> {
        if !is_child_family(family) {
            return Err(StoreError::UnsupportedObservationFamily(
                evidence_family_name(family).to_owned(),
            ));
        }
        if expected_item_count.is_none() && !allow_stored_count {
            return Ok(false);
        }
        let source_clock = normalize_source_clock(source_clock)?;
        let source_fields = source_clock_columns(&source_clock)?;
        let mut connection = self.reader.acquire().await?;
        let row_id = thread_row_id(&mut connection, thread).await?;
        let row = sqlx::query(
            "SELECT source_clock_state, source_clock_raw, source_clock_us, state_json FROM family_coverage WHERE thread_id = ? AND family = ?",
        )
        .bind(row_id)
        .bind(evidence_family_name(family))
        .fetch_optional(&mut *connection)
        .await?;
        let Some(row) = row else {
            return Ok(false);
        };
        let stored_state: String = row.try_get("source_clock_state")?;
        let stored_raw: String = row.try_get("source_clock_raw")?;
        let stored_microseconds: Option<i64> = row.try_get("source_clock_us")?;
        if stored_state != source_fields.state
            || stored_raw != source_fields.raw
            || stored_microseconds != source_fields.unix_microseconds
        {
            return Ok(false);
        }
        if let Some(expected_head_sha) = expected_head_sha {
            let stored_head_sha: Option<String> = sqlx::query_scalar(
                "SELECT head_sha FROM thread_family_head_contexts WHERE thread_id = ? AND family = ?",
            )
            .bind(row_id)
            .bind(evidence_family_name(family))
            .fetch_optional(&mut *connection)
            .await?;
            if stored_head_sha.as_deref() != Some(expected_head_sha.as_str()) {
                return Ok(false);
            }
        }
        let state_json: String = row.try_get("state_json")?;
        let state: CoverageState = serde_json::from_str(&state_json)?;
        let CoverageState::Complete { item_count, .. } = state else {
            return Ok(false);
        };
        if expected_item_count.is_some_and(|expected| item_count != expected) {
            return Ok(false);
        }
        let member_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM thread_family_membership WHERE thread_id = ? AND family = ?",
        )
        .bind(row_id)
        .bind(evidence_family_name(family))
        .fetch_one(&mut *connection)
        .await?;
        let member_count =
            u64::try_from(member_count).map_err(|_| StoreError::InvalidStoredSequence)?;
        Ok(member_count == item_count)
    }
}

async fn load_staged_pages(
    connection: &mut SqliteConnection,
    thread_row_id: i64,
    family: &str,
    sequence: i64,
) -> Result<Vec<StagedPage>, StoreError> {
    let rows = sqlx::query(
        "SELECT page_index, payload_json FROM observation_staging_pages WHERE thread_id = ? AND family = ? AND sequence = ? ORDER BY page_index",
    )
    .bind(thread_row_id)
    .bind(family)
    .bind(sequence)
    .fetch_all(&mut *connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            let index: i64 = row.try_get("page_index")?;
            let payload_json: String = row.try_get("payload_json")?;
            Ok(StagedPage {
                index,
                items: serde_json::from_str(&payload_json)?,
            })
        })
        .collect()
}

fn count_staged_items(pages: &[StagedPage]) -> Result<u64, StoreError> {
    pages.iter().try_fold(0_u64, |count, page| {
        let item_count =
            u64::try_from(page.items.len()).map_err(|_| StoreError::IntegerOutOfRange)?;
        count
            .checked_add(item_count)
            .ok_or(StoreError::IntegerOutOfRange)
    })
}

fn validate_page_set(pages: &[StagedPage], expected_pages: u32) -> Result<(), StoreError> {
    let found = u32::try_from(pages.len()).map_err(|_| StoreError::IntegerOutOfRange)?;
    if found != expected_pages {
        return Err(StoreError::IncompletePageSet {
            expected: expected_pages,
            found,
        });
    }
    for (index, page) in pages.iter().enumerate() {
        let expected_index = i64::try_from(index).map_err(|_| StoreError::IntegerOutOfRange)?;
        if page.index != expected_index {
            return Err(StoreError::IncompletePageSet {
                expected: expected_pages,
                found,
            });
        }
    }
    Ok(())
}

fn merge_staged_items(
    pages: &[StagedPage],
) -> Result<BTreeMap<String, StagedItem<serde_json::Value>>, StoreError> {
    let mut items = BTreeMap::new();
    for item in pages.iter().flat_map(|page| &page.items) {
        let key = item.id.as_str().to_owned();
        if let Some(existing) = items.get(&key) {
            if existing != item {
                return Err(StoreError::StagedItemConflict);
            }
        } else {
            items.insert(key, item.clone());
        }
    }
    Ok(items)
}
