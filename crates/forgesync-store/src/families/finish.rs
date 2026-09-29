//! Child-family finish operations.

use sqlx::Row;

use super::{
    Archive, ArchiveLeaseToken, ChildFamilyObservation, CollectionCompleteness, CoverageState,
    EvidenceFamily, FamilyObservationResult, ObservationDisposition, ObservationSequence,
    StoreError, ThreadId, UtcTimestamp, count_staged_items, evidence_family_name, is_child_family,
    load_staged_pages, merge_staged_items, require_active_archive_lease, source_clock_columns,
    source_clock_from_columns, thread_row_id, to_sql_sequence, validate_page_set, write_coverage,
};

impl Archive {
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

    /// Atomically promotes complete staged membership or records incomplete coverage.
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
}
