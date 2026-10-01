//! Family coverage projection and the shared child-family freshness rule.
//!
//! Coverage is never inferred from child counts: zero comments in a complete collection and zero
//! stored comments after a failed collection mean different things. A complete stored state can
//! still be displayed as stale when the parent clock, comment count, or pull-request head moved.

use std::collections::HashMap;

use forgesync_core::content::{Discussion, PullRequestMetadata, ThreadKind};
use forgesync_core::coverage::{Coverage, CoverageState, EvidenceFamily};
use forgesync_core::observation::SourceClock;
use sqlx::sqlite::SqliteRow;
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::error::StoreError;
use crate::sql::{
    ALL_FAMILIES, SourceClockColumns, is_pull_request_family, parse_family, push_bound_list,
    source_clock_columns,
};

/// Recorded completeness with the source clock and review head it was acquired against.
pub struct StoredCoverage {
    pub state: CoverageState,
    pub clock: SourceClockColumns,
    /// Pull-request head recorded with complete review evidence.
    pub head_sha: Option<String>,
    /// Current pull-request head from metadata membership, for projection only.
    pub current_head_sha: Option<String>,
}

impl StoredCoverage {
    /// Decodes a `family_coverage` row joined with its head context as `head_sha`.
    pub fn from_row(row: &SqliteRow) -> Result<Self, StoreError> {
        Ok(Self {
            state: serde_json::from_str(&row.try_get::<String, _>("state_json")?)?,
            clock: SourceClockColumns {
                state: match row.try_get::<String, _>("source_clock_state")?.as_str() {
                    "missing" => "missing",
                    "valid" => "valid",
                    "invalid" => "invalid",
                    _ => return Err(StoreError::Corrupt("archive_coverage_invalid")),
                },
                raw: row.try_get("source_clock_raw")?,
                unix_microseconds: row.try_get("source_clock_us")?,
            },
            head_sha: row.try_get("head_sha")?,
            current_head_sha: None,
        })
    }
}

/// The evidence, besides an identical source clock, that keeps child coverage current.
#[derive(Clone, Copy)]
pub enum ChildExpectation<'a> {
    /// Parent-reported member count; an unknown count never matches complete coverage.
    Count(Option<u64>),
    /// Pull-request head that review evidence must have been acquired against.
    Head(Option<&'a str>),
}

/// Whether stored child coverage still describes the expected source clock and count or head.
pub fn child_coverage_matches(
    stored: &StoredCoverage,
    source: &SourceClockColumns,
    expectation: ChildExpectation<'_>,
) -> bool {
    stored.clock == *source
        && match expectation {
            ChildExpectation::Count(expected) => match stored.state {
                CoverageState::Complete { item_count, .. } => expected == Some(item_count),
                _ => true,
            },
            ChildExpectation::Head(expected) => stored.head_sha.as_deref() == expected,
        }
}

/// Loads coverage for the given threads, keyed by thread row and family.
pub async fn load_thread_coverage(
    pool: &sqlx::SqlitePool,
    thread_ids: &[i64],
) -> Result<HashMap<i64, HashMap<EvidenceFamily, StoredCoverage>>, StoreError> {
    if thread_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let current_heads = current_review_heads(pool, thread_ids).await?;

    let mut statement = QueryBuilder::<Sqlite>::new(
        "SELECT c.thread_id, c.family, c.state_json, c.source_clock_state, c.source_clock_raw, c.source_clock_us, h.head_sha FROM family_coverage c LEFT JOIN thread_family_head_contexts h ON h.thread_id = c.thread_id AND h.family = c.family WHERE c.thread_id IN (",
    );
    push_bound_list(&mut statement, thread_ids);
    statement.push(")");
    let rows = statement.build().fetch_all(pool).await?;
    let mut coverage: HashMap<i64, HashMap<EvidenceFamily, StoredCoverage>> =
        HashMap::with_capacity(thread_ids.len());
    for row in rows {
        let thread_id: i64 = row.try_get("thread_id")?;
        let family = parse_family(&row.try_get::<String, _>("family")?)
            .ok_or(StoreError::Corrupt("archive_coverage_invalid"))?;
        let mut stored = StoredCoverage::from_row(&row)?;
        stored.current_head_sha = current_heads.get(&thread_id).cloned();
        coverage
            .entry(thread_id)
            .or_default()
            .insert(family, stored);
    }
    Ok(coverage)
}

/// Reads current pull-request heads from metadata membership.
async fn current_review_heads(
    pool: &sqlx::SqlitePool,
    thread_ids: &[i64],
) -> Result<HashMap<i64, String>, StoreError> {
    let mut statement = QueryBuilder::<Sqlite>::new(
        "SELECT thread_id, payload_json FROM thread_family_membership WHERE family = 'pull_request_metadata' AND thread_id IN (",
    );
    push_bound_list(&mut statement, thread_ids);
    statement.push(")");
    let rows = statement.build().fetch_all(pool).await?;
    let mut current_heads = HashMap::with_capacity(rows.len());
    for row in rows {
        let thread_id: i64 = row.try_get("thread_id")?;
        let metadata: PullRequestMetadata =
            serde_json::from_str(&row.try_get::<String, _>("payload_json")?)?;
        current_heads.insert(thread_id, metadata.head.sha.as_str().to_owned());
    }
    Ok(current_heads)
}

/// Projects stored coverage onto the families relevant to this discussion kind, reporting absent
/// rows as missing.
pub fn coverage_for_kind(
    discussion: &Discussion,
    stored: Option<&HashMap<EvidenceFamily, StoredCoverage>>,
) -> Vec<Coverage> {
    ALL_FAMILIES
        .into_iter()
        .filter(|family| {
            discussion.kind == ThreadKind::PullRequest || !is_pull_request_family(*family)
        })
        .map(|family| {
            let item = stored.and_then(|coverage| coverage.get(&family));
            let state = item
                .map(|item| item.state.clone())
                .unwrap_or(CoverageState::Missing);
            let coverage = Coverage::new(family, state);
            if is_stale(discussion, family, item) {
                coverage.mark_stale()
            } else {
                coverage
            }
        })
        .collect()
}

/// Marks recorded child evidence stale when it no longer matches the current parent discussion.
fn is_stale(
    discussion: &Discussion,
    family: EvidenceFamily,
    stored: Option<&StoredCoverage>,
) -> bool {
    let Some(stored) = stored else {
        return false;
    };
    if matches!(stored.state, CoverageState::Missing) {
        return false;
    }
    let expectation = match family {
        EvidenceFamily::Comments => ChildExpectation::Count(
            discussion
                .provider_data
                .get("comments")
                .and_then(serde_json::Value::as_u64),
        ),
        EvidenceFamily::Reviews | EvidenceFamily::ReviewThreads => {
            ChildExpectation::Head(stored.current_head_sha.as_deref())
        }
        EvidenceFamily::Threads | EvidenceFamily::PullRequestMetadata => return false,
    };
    let source = source_clock_columns(&SourceClock::Valid(discussion.updated_at));
    !child_coverage_matches(stored, &source, expectation)
}
