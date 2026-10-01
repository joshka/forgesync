//! Finalizing a staged child-family collection.
//!
//! A complete finish validates the whole page set before replacing canonical membership. An
//! incomplete finish records coverage only, preserving the last complete membership. Staged pages
//! from earlier calls stay durable when finalization fails.

use std::collections::BTreeMap;

use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::observation::CollectionCompleteness;
use sqlx::{Row, SqliteConnection};

use crate::archive::Archive;
use crate::error::StoreError;
use crate::families::ChildFamilyObservation;
use crate::families::query::require_child_family;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::observations::{FamilyObservationResult, ObservationDisposition, StagedItem};
use crate::sql::{
    count_from_sql, source_clock_columns, source_clock_from_columns, thread_row_id, to_sql_integer,
    to_sql_sequence, write_coverage,
};

/// One staged page decoded for page-set validation and membership merging.
struct StagedPage {
    index: i64,
    items: Vec<StagedItem<serde_json::Value>>,
}

impl Archive {
    /// Finalizes a staged child collection under the archive writer fence.
    ///
    /// Inspect the returned disposition: a superseded reservation is skipped rather than failing,
    /// and finishing an already complete generation replays its result.
    pub async fn finish_child_family_observation_fenced(
        &self,
        observation: ChildFamilyObservation<'_>,
        token: &ArchiveLeaseToken,
    ) -> Result<FamilyObservationResult, StoreError> {
        validate_observation(&observation)?;
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let sequence = to_sql_sequence(observation.sequence)?;
        let family = observation.family.as_str();
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let thread = thread_row_id(&mut transaction, observation.thread).await?;

        let current: Option<i64> = sqlx::query_scalar(
            "SELECT sequence FROM thread_family_reservations WHERE thread_id = ? AND family = ?",
        )
        .bind(thread)
        .bind(family)
        .fetch_optional(&mut *transaction)
        .await?;
        if current != Some(sequence) {
            return Ok(FamilyObservationResult {
                disposition: ObservationDisposition::Skipped,
                item_count: 0,
            });
        }
        let generation = sqlx::query(
            "SELECT source_clock_state, source_clock_raw, source_clock_us, status, item_count FROM observation_generations WHERE thread_id = ? AND family = ? AND sequence = ?",
        )
        .bind(thread)
        .bind(family)
        .bind(sequence)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(StoreError::ObservationGenerationMissing)?;
        if generation.try_get::<String, _>("status")? == "complete" {
            return Ok(FamilyObservationResult {
                disposition: ObservationDisposition::Replayed,
                item_count: count_from_sql(generation.try_get("item_count")?)?,
            });
        }
        // Coverage keeps the clock that won the reservation, not a caller-supplied revision.
        let source_clock = source_clock_columns(&source_clock_from_columns(
            &generation.try_get::<String, _>("source_clock_state")?,
            &generation.try_get::<String, _>("source_clock_raw")?,
            generation.try_get("source_clock_us")?,
        )?);
        let pages = load_staged_pages(&mut transaction, thread, family, sequence).await?;
        // Received entries include identities repeated across overlapping pages.
        let staged_count = pages.iter().map(|page| page.items.len()).sum::<usize>();
        let staged_count =
            u64::try_from(staged_count).map_err(|_| StoreError::IntegerOutOfRange)?;

        let (state, status, received, item_count) = match observation.completeness {
            CollectionCompleteness::Complete => {
                let expected = observation
                    .expected_pages
                    .ok_or(StoreError::MissingExpectedPageCount)?;
                validate_page_set(&pages, expected)?;
                let items = merge_staged_items(&pages)?;
                sqlx::query(
                    "DELETE FROM thread_family_membership WHERE thread_id = ? AND family = ?",
                )
                .bind(thread)
                .bind(family)
                .execute(&mut *transaction)
                .await?;
                for item in items.values() {
                    sqlx::query(
                        "INSERT INTO thread_family_membership (thread_id, family, provider_id, payload_json, sequence) VALUES (?, ?, ?, ?, ?)",
                    )
                    .bind(thread)
                    .bind(family)
                    .bind(item.id.as_str())
                    .bind(serde_json::to_string(&item.payload)?)
                    .bind(sequence)
                    .execute(&mut *transaction)
                    .await?;
                }
                let item_count =
                    u64::try_from(items.len()).map_err(|_| StoreError::IntegerOutOfRange)?;
                let state = CoverageState::Complete {
                    observed_at: observation.observed_at,
                    sequence: observation.sequence,
                    item_count,
                };
                (state, "complete", staged_count, item_count)
            }
            CollectionCompleteness::Incomplete {
                reason,
                received_items,
            } => {
                if *received_items != staged_count {
                    return Err(StoreError::InvalidCollectionCompleteness);
                }
                let state = CoverageState::Incomplete {
                    observed_at: observation.observed_at,
                    sequence: observation.sequence,
                    reason: *reason,
                    received_items: *received_items,
                    failure: None,
                };
                (state, "incomplete", *received_items, staged_count)
            }
        };
        write_coverage(
            &mut transaction,
            thread,
            observation.family,
            &source_clock,
            &state,
        )
        .await?;
        // Head context references the coverage row, so it is written after coverage.
        if let (CollectionCompleteness::Complete, Some(head)) =
            (observation.completeness, observation.head_sha)
        {
            sqlx::query(
                "INSERT INTO thread_family_head_contexts (thread_id, family, head_sha, sequence) VALUES (?, ?, ?, ?) ON CONFLICT (thread_id, family) DO UPDATE SET head_sha = excluded.head_sha, sequence = excluded.sequence",
            )
            .bind(thread)
            .bind(family)
            .bind(head.as_str())
            .bind(sequence)
            .execute(&mut *transaction)
            .await?;
        }
        sqlx::query(
            "UPDATE observation_generations SET status = ?, received_items = ?, item_count = ? WHERE thread_id = ? AND family = ? AND sequence = ?",
        )
        .bind(status)
        .bind(to_sql_integer(received)?)
        .bind(to_sql_integer(item_count)?)
        .bind(thread)
        .bind(family)
        .bind(sequence)
        .execute(&mut *transaction)
        .await?;
        if status == "complete" {
            // Obsolete attempts are only discarded once a complete membership replaces them.
            sqlx::query("DELETE FROM observation_staging_pages WHERE thread_id = ? AND family = ?")
                .bind(thread)
                .bind(family)
                .execute(&mut *transaction)
                .await?;
            sqlx::query(
                "DELETE FROM observation_generations WHERE thread_id = ? AND family = ? AND sequence <> ?",
            )
            .bind(thread)
            .bind(family)
            .bind(sequence)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(FamilyObservationResult {
            disposition: ObservationDisposition::Applied,
            item_count,
        })
    }
}

/// Checks completeness and head requirements before any transaction begins.
fn validate_observation(observation: &ChildFamilyObservation<'_>) -> Result<(), StoreError> {
    require_child_family(observation.family)?;
    match observation.completeness {
        CollectionCompleteness::Complete if observation.expected_pages.is_none() => {
            return Err(StoreError::MissingExpectedPageCount);
        }
        CollectionCompleteness::Incomplete { .. } if observation.expected_pages.is_some() => {
            return Err(StoreError::InvalidCollectionCompleteness);
        }
        _ => {}
    }
    let head_bound_family = matches!(
        observation.family,
        EvidenceFamily::Reviews | EvidenceFamily::ReviewThreads
    );
    if observation.head_sha.is_some() && !head_bound_family {
        return Err(StoreError::UnexpectedPullRequestHeadContext);
    }
    if head_bound_family
        && matches!(observation.completeness, CollectionCompleteness::Complete)
        && observation.head_sha.is_none()
    {
        return Err(StoreError::MissingPullRequestHeadContext);
    }
    Ok(())
}

/// Loads pages for the reserved generation in page-number order.
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
            Ok(StagedPage {
                index: row.try_get("page_index")?,
                items: serde_json::from_str(&row.try_get::<String, _>("payload_json")?)?,
            })
        })
        .collect()
}

/// Requires exactly the pages `0..expected_pages` before a collection can become complete.
fn validate_page_set(pages: &[StagedPage], expected_pages: u32) -> Result<(), StoreError> {
    let found = u32::try_from(pages.len()).map_err(|_| StoreError::IntegerOutOfRange)?;
    let contiguous = pages
        .iter()
        .enumerate()
        .all(|(position, page)| i64::try_from(position) == Ok(page.index));
    if found != expected_pages || !contiguous {
        return Err(StoreError::IncompletePageSet {
            expected: expected_pages,
            found,
        });
    }
    Ok(())
}

/// Merges pages by provider ID, rejecting conflicting duplicates within one generation.
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
