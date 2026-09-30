//! # Read family completeness for a discussion
//!
//! These helpers load coverage records and map them into summaries by evidence family. The result
//! lets a thread detail report what has been collected and what remains incomplete.
//!
//! Coverage is not inferred from child counts. Zero comments in a complete collection and zero
//! stored comments after a failed collection mean different things; the explicit coverage row
//! preserves that distinction for CLI and TUI readers.

use std::collections::HashMap;

use forgesync_core::content::{Discussion, PullRequestMetadata, ThreadKind};
use forgesync_core::coverage::{Coverage, CoverageState, EvidenceFamily};
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::error::StoreError;
use crate::reads::{ALL_FAMILIES, StoredCoverage};

/// Loads coverage with both the recorded review head and current PR head so callers can mark
/// review evidence stale without rewriting the stored collection.
pub async fn load_thread_coverage(
    pool: &sqlx::SqlitePool,
    thread_ids: &[i64],
) -> Result<HashMap<i64, HashMap<EvidenceFamily, StoredCoverage>>, StoreError> {
    if thread_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut metadata_statement = QueryBuilder::<Sqlite>::new(
        "SELECT thread_id, payload_json FROM thread_family_membership WHERE family = 'pull_request_metadata' AND thread_id IN (",
    );
    for (index, thread_id) in thread_ids.iter().enumerate() {
        if index > 0 {
            metadata_statement.push(", ");
        }
        metadata_statement.push_bind(thread_id);
    }
    metadata_statement.push(")");
    let metadata_rows = metadata_statement.build().fetch_all(pool).await?;
    let mut current_heads = HashMap::with_capacity(metadata_rows.len());
    for row in metadata_rows {
        let thread_id: i64 = row.try_get("thread_id")?;
        let payload_json: String = row.try_get("payload_json")?;
        let metadata: PullRequestMetadata = serde_json::from_str(&payload_json)?;
        current_heads.insert(thread_id, metadata.head.sha.as_str().to_owned());
    }

    let mut statement = QueryBuilder::<Sqlite>::new(
        "SELECT c.thread_id, c.family, c.state_json, c.source_clock_state, c.source_clock_us, h.head_sha AS snapshot_head_sha FROM family_coverage c LEFT JOIN thread_family_head_contexts h ON h.thread_id = c.thread_id AND h.family = c.family WHERE c.thread_id IN (",
    );
    for (index, thread_id) in thread_ids.iter().enumerate() {
        if index > 0 {
            statement.push(", ");
        }
        statement.push_bind(thread_id);
    }
    statement.push(")");
    let rows = statement.build().fetch_all(pool).await?;
    let mut coverage = HashMap::with_capacity(thread_ids.len());
    for row in rows {
        let thread_id: i64 = row.try_get("thread_id")?;
        let family: String = row.try_get("family")?;
        let state_json: String = row.try_get("state_json")?;
        let source_clock_state: String = row.try_get("source_clock_state")?;
        let source_clock_us: Option<i64> = row.try_get("source_clock_us")?;
        let snapshot_head_sha: Option<String> = row.try_get("snapshot_head_sha")?;
        let family = parse_evidence_family(&family)?;
        let state = serde_json::from_str(&state_json)?;
        coverage
            .entry(thread_id)
            .or_insert_with(HashMap::new)
            .insert(
                family,
                StoredCoverage {
                    state,
                    source_clock_state,
                    source_clock_us,
                    snapshot_head_sha,
                    current_head_sha: current_heads.get(&thread_id).cloned(),
                },
            );
    }
    Ok(coverage)
}

/// Projects stored family coverage onto the families relevant to this discussion kind. Missing
/// rows remain visible as missing evidence instead of disappearing from inspection output.
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
            Coverage::new(family, state).with_stale(is_stale(discussion, family, item))
        })
        .collect()
}

/// Marks child evidence stale when its parent update clock changes, or when comments no longer
/// match the provider count or review evidence belongs to an earlier pull-request head.
fn is_stale(
    discussion: &Discussion,
    family: EvidenceFamily,
    stored: Option<&StoredCoverage>,
) -> bool {
    if !matches!(
        family,
        EvidenceFamily::Comments | EvidenceFamily::Reviews | EvidenceFamily::ReviewThreads
    ) {
        return false;
    }
    let Some(stored) = stored else {
        return false;
    };
    if matches!(stored.state, CoverageState::Missing) {
        return false;
    }

    let parent_clock_matches = stored.source_clock_state == "valid"
        && stored.source_clock_us == Some(discussion.updated_at.unix_microseconds());
    if !parent_clock_matches {
        return true;
    }
    match family {
        EvidenceFamily::Comments => match stored.state {
            CoverageState::Complete { item_count, .. } => {
                comment_count(discussion) != Some(item_count)
            }
            _ => false,
        },
        EvidenceFamily::Reviews | EvidenceFamily::ReviewThreads => {
            stored.snapshot_head_sha != stored.current_head_sha
        }
        _ => false,
    }
}

/// Reads the current source comment count for coverage display.
fn comment_count(discussion: &Discussion) -> Option<u64> {
    discussion
        .provider_data
        .get("comments")
        .and_then(serde_json::Value::as_u64)
}

/// Excludes issue rows from pull-request-only coverage totals.
pub fn is_pull_request_family(family: EvidenceFamily) -> bool {
    matches!(
        family,
        EvidenceFamily::PullRequestMetadata
            | EvidenceFamily::Reviews
            | EvidenceFamily::ReviewThreads
    )
}

/// Maps one family to its persisted archive label.
pub fn evidence_family_name(family: EvidenceFamily) -> &'static str {
    match family {
        EvidenceFamily::Threads => "threads",
        EvidenceFamily::Comments => "comments",
        EvidenceFamily::PullRequestMetadata => "pull_request_metadata",
        EvidenceFamily::Reviews => "reviews",
        EvidenceFamily::ReviewThreads => "review_threads",
    }
}

/// Rejects stored family labels unsupported by this binary.
fn parse_evidence_family(value: &str) -> Result<EvidenceFamily, StoreError> {
    match value {
        "threads" => Ok(EvidenceFamily::Threads),
        "comments" => Ok(EvidenceFamily::Comments),
        "pull_request_metadata" => Ok(EvidenceFamily::PullRequestMetadata),
        "reviews" => Ok(EvidenceFamily::Reviews),
        "review_threads" => Ok(EvidenceFamily::ReviewThreads),
        _ => Err(StoreError::InvalidStoredCoverage),
    }
}
