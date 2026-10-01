//! Child-family membership reads and reuse decisions.
//!
//! Reuse checks read coverage and membership separately; they do not freeze a snapshot or
//! authorize a later write.

use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::identity::{CommitSha, ProviderId, ThreadId};
use forgesync_core::observation::SourceClock;
use serde::de::DeserializeOwned;
use sqlx::{Row, SqliteExecutor};

use crate::archive::Archive;
use crate::coverage_projection::{ChildExpectation, StoredCoverage, child_coverage_matches};
use crate::error::StoreError;
use crate::observations::StagedItem;
use crate::sql::{count_from_sql, is_child_family, source_clock_columns, thread_row_id};

impl Archive {
    /// Returns the canonical complete membership for one thread family in provider-ID order.
    ///
    /// Provisional staging rows are excluded. An empty result alone does not establish complete
    /// coverage.
    pub async fn child_family_members<T>(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
    ) -> Result<Vec<StagedItem<T>>, StoreError>
    where
        T: DeserializeOwned,
    {
        require_child_family(family)?;
        let mut connection = self.reader.acquire().await?;
        let thread_row_id = thread_row_id(&mut connection, thread).await?;
        family_members(&mut *connection, thread_row_id, family).await
    }

    /// Returns whether complete stored membership matches the parent clock and expected count.
    ///
    /// An unknown count deliberately forces a refresh.
    pub async fn child_family_is_current(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        source_clock: &SourceClock,
        expected_item_count: Option<u64>,
    ) -> Result<bool, StoreError> {
        if expected_item_count.is_none() {
            require_child_family(family)?;
            return Ok(false);
        }
        self.child_family_matches(
            thread,
            family,
            source_clock,
            ChildExpectation::Count(expected_item_count),
        )
        .await
    }

    /// Returns whether complete review evidence matches the parent clock and pull-request head.
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
        self.child_family_matches(
            thread,
            family,
            source_clock,
            ChildExpectation::Head(Some(head_sha.as_str())),
        )
        .await
    }

    /// Requires complete coverage matching the expectation and agreeing with canonical membership.
    async fn child_family_matches(
        &self,
        thread: &ThreadId,
        family: EvidenceFamily,
        source_clock: &SourceClock,
        expectation: ChildExpectation<'_>,
    ) -> Result<bool, StoreError> {
        require_child_family(family)?;
        let source = source_clock_columns(source_clock);
        let mut connection = self.reader.acquire().await?;
        let thread_row_id = thread_row_id(&mut connection, thread).await?;
        let row = sqlx::query(
            "SELECT c.state_json, c.source_clock_state, c.source_clock_raw, c.source_clock_us, h.head_sha FROM family_coverage c LEFT JOIN thread_family_head_contexts h ON h.thread_id = c.thread_id AND h.family = c.family WHERE c.thread_id = ? AND c.family = ?",
        )
        .bind(thread_row_id)
        .bind(family.as_str())
        .fetch_optional(&mut *connection)
        .await?;
        let Some(row) = row else {
            return Ok(false);
        };
        let stored = StoredCoverage::from_row(&row)?;
        let CoverageState::Complete { item_count, .. } = stored.state else {
            return Ok(false);
        };
        if !child_coverage_matches(&stored, &source, expectation) {
            return Ok(false);
        }
        let members: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM thread_family_membership WHERE thread_id = ? AND family = ?",
        )
        .bind(thread_row_id)
        .bind(family.as_str())
        .fetch_one(&mut *connection)
        .await?;
        Ok(count_from_sql(members)? == item_count)
    }
}

/// Loads current complete family membership for a discussion row in provider-ID order.
pub(crate) async fn family_members<'e, T>(
    executor: impl SqliteExecutor<'e>,
    thread_row_id: i64,
    family: EvidenceFamily,
) -> Result<Vec<StagedItem<T>>, StoreError>
where
    T: DeserializeOwned,
{
    let rows = sqlx::query(
        "SELECT provider_id, payload_json FROM thread_family_membership WHERE thread_id = ? AND family = ? ORDER BY provider_id",
    )
    .bind(thread_row_id)
    .bind(family.as_str())
    .fetch_all(executor)
    .await?;
    rows.into_iter()
        .map(|row| {
            let id = ProviderId::new(row.try_get::<String, _>("provider_id")?)
                .map_err(|_| StoreError::Corrupt("archive_provider_id_invalid"))?;
            Ok(StagedItem {
                id,
                payload: serde_json::from_str(&row.try_get::<String, _>("payload_json")?)?,
            })
        })
        .collect()
}

/// Rejects the parent-thread family on child-family operations.
pub(crate) fn require_child_family(family: EvidenceFamily) -> Result<(), StoreError> {
    if is_child_family(family) {
        Ok(())
    } else {
        Err(StoreError::UnsupportedObservationFamily(
            family.as_str().to_owned(),
        ))
    }
}
