//! # Read child-family collection state
//!
//! These `Archive` methods expose the coverage and staged or completed state needed by inspection
//! and retry workflows. They translate database records into domain-facing results instead of
//! making the engine reason about staging tables.
//!
//! Keep a family's coverage independent from parent-thread coverage: a current parent snapshot
//! does not prove that comments, reviews, or review threads were collected successfully.
//! `MembershipExpectation` distinguishes parent-reported counts from head-bound review evidence.
//! `FamilyFreshness` evaluates those expectations against coverage and canonical rows without
//! changing the archive. Membership reads decode payloads separately from the reuse decision.

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
            MembershipExpectation::Reported(expected_item_count),
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
        self.child_family_is_current_inner(
            thread,
            family,
            source_clock,
            MembershipExpectation::ForHead(head_sha),
        )
        .await
    }

    /// Checks whether staged child evidence still belongs to the current parent.
    async fn child_family_is_current_inner(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        source_clock: &SourceClock,
        expectation: MembershipExpectation<'_>,
    ) -> Result<bool, StoreError> {
        if !is_child_family(family) {
            return Err(StoreError::UnsupportedObservationFamily(
                evidence_family_name(family).to_owned(),
            ));
        }
        if matches!(expectation, MembershipExpectation::Reported(None)) {
            return Ok(false);
        }
        let source_clock = normalize_source_clock(source_clock)?;
        let source_fields = source_clock_columns(&source_clock)?;
        let mut connection = self.reader.acquire().await?;
        let row_id = thread_row_id(&mut connection, thread).await?;
        let freshness = FamilyFreshness {
            thread: row_id,
            family: evidence_family_name(family),
            source: source_fields,
            expectation,
        };
        freshness.current(&mut connection).await
    }
}

/// Reuse eligibility for one family, using independent source, head, and membership evidence.
///
/// A current parent does not make a child collection current. This read requires the exact source
/// clock, complete coverage, a count that agrees with canonical membership, and (for review
/// families) the expected pull-request head. No check here changes coverage or staged generations.
struct FamilyFreshness<'a> {
    thread: i64,
    family: &'static str,
    source: crate::observations::SourceClockColumns,
    expectation: MembershipExpectation<'a>,
}

impl FamilyFreshness<'_> {
    /// Evaluates complete coverage before comparing the durable member count.
    async fn current(&self, connection: &mut sqlx::SqliteConnection) -> Result<bool, StoreError> {
        let row = sqlx::query(
            "SELECT source_clock_state, source_clock_raw, source_clock_us, state_json FROM family_coverage WHERE thread_id = ? AND family = ?",
        )
        .bind(self.thread).bind(self.family).fetch_optional(&mut *connection).await?;
        let Some(row) = row else {
            return Ok(false);
        };
        if !self.clock_matches(&row)? {
            return Ok(false);
        }
        if !self.head_matches(connection).await? {
            return Ok(false);
        }
        let state_json: String = row.try_get("state_json")?;
        let state: CoverageState = serde_json::from_str(&state_json)?;
        let CoverageState::Complete { item_count, .. } = state else {
            return Ok(false);
        };
        if let MembershipExpectation::Reported(Some(expected)) = self.expectation
            && item_count != expected
        {
            return Ok(false);
        }
        self.members_match(connection, item_count).await
    }

    /// Compares all source-clock columns so malformed and unknown clocks retain their identity.
    fn clock_matches(&self, row: &sqlx::sqlite::SqliteRow) -> Result<bool, StoreError> {
        let state: String = row.try_get("source_clock_state")?;
        let raw: String = row.try_get("source_clock_raw")?;
        let microseconds: Option<i64> = row.try_get("source_clock_us")?;
        Ok(state == self.source.state
            && raw == self.source.raw
            && microseconds == self.source.unix_microseconds)
    }

    /// Requires the stored review head only for families whose expected count comes from coverage.
    async fn head_matches(
        &self,
        connection: &mut sqlx::SqliteConnection,
    ) -> Result<bool, StoreError> {
        let MembershipExpectation::ForHead(expected) = self.expectation else {
            return Ok(true);
        };
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT head_sha FROM thread_family_head_contexts WHERE thread_id = ? AND family = ?",
        )
        .bind(self.thread)
        .bind(self.family)
        .fetch_optional(&mut *connection)
        .await?;
        Ok(stored.as_deref() == Some(expected.as_str()))
    }

    /// Rejects coverage whose recorded complete count no longer agrees with canonical membership.
    async fn members_match(
        &self,
        connection: &mut sqlx::SqliteConnection,
        expected: u64,
    ) -> Result<bool, StoreError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM thread_family_membership WHERE thread_id = ? AND family = ?",
        )
        .bind(self.thread)
        .bind(self.family)
        .fetch_one(&mut *connection)
        .await?;
        let count = u64::try_from(count).map_err(|_| StoreError::InvalidStoredSequence)?;
        Ok(count == expected)
    }
}

/// The independent evidence that makes a stored complete membership eligible for reuse.
///
/// Parent-reported child counts must be known and match coverage. Pull-request review families
/// instead require a matching head and validate their coverage count against canonical membership,
/// because the parent issue representation does not report those family counts.
#[derive(Clone, Copy)]
enum MembershipExpectation<'a> {
    /// Count supplied by parent metadata; unknown counts require acquisition.
    Reported(Option<u64>),
    /// Review evidence tied to the current pull-request head.
    ForHead(&'a CommitSha),
}
