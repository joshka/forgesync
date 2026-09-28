use std::collections::BTreeMap;

use forgesync_core::{
    CollectionCompleteness, CoverageState, EvidenceFamily, ObservationSequence, SourceClock,
    ThreadId, UtcTimestamp,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::{Row, SqliteConnection};

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

    /// Commits a complete membership snapshot or records an incomplete attempt without replacing it.
    pub async fn finish_child_family_observation(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        sequence: ObservationSequence,
        observed_at: UtcTimestamp,
        completeness: &CollectionCompleteness,
        expected_pages: Option<u32>,
    ) -> Result<FamilyObservationResult, StoreError> {
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

        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let family_name = evidence_family_name(family);
        let sequence_value = to_sql_sequence(sequence)?;
        let mut transaction = writer.begin().await?;
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
