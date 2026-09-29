//! # Read child-family collection state
//!
//! These `Archive` methods expose the coverage and staged or completed state needed by inspection
//! and retry workflows. They translate database records into domain-facing results instead of
//! making the engine reason about staging tables.
//!
//! Keep a family's coverage independent from parent-thread coverage: a current parent snapshot
//! does not prove that comments, reviews, or review threads were collected successfully.

use sqlx::Row;

use super::{
    Archive, CommitSha, CoverageState, DeserializeOwned, EvidenceFamily, SourceClock, StagedItem,
    StoreError, ThreadId, evidence_family_name, is_child_family, normalize_source_clock,
    source_clock_columns, thread_row_id,
};

impl Archive {
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

    /// Checks whether staged child evidence still belongs to the current parent.
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
