//! # Exact vector scoring for local semantic search
//!
//! [`cosine_similarity`] compares two validated vectors using scaled f64 arithmetic. The internal
//! `ScoredThread` pairs a retained discussion projection with its best comparable chunk score.
//! The semantic search workflow supplies candidate documents selected by the store; this module
//! performs arithmetic and deterministic ordering without querying or acquiring evidence.
//!
//! Page scoring rejects a whole document if any chunk has the wrong dimension, then retains only
//! documents whose best score is finite and strictly positive. Model, endpoint, recipe, freshness,
//! and chunk-set eligibility belong to the caller/store selection boundary, not this arithmetic.
//! Cancellation is checked between documents, not during an individual vector calculation.
//!
//! Relevance ordering uses descending similarity; explicit updated/created ordering uses source
//! timestamps instead. Every policy breaks primary ties by stable discussion identity. Page merging
//! retains the best bounded prefix under the same policy, so callers must use a consistent sort
//! and retention limit across pages. It does not deduplicate repeated discussion identities.
//!
//! The public arithmetic helper is independently usable. Candidate scoring and ordering helpers
//! remain internal because they implement the engine's retrieval policy rather than a separate
//! public search interface. Nearby tests use fixed projections to make numerical and tie behavior
//! visible without archive or service setup.

use std::cmp::Ordering;

use forgesync_core::embedding::EmbeddingVector;
use forgesync_store::embeddings::EmbeddingSearchDocument;
use forgesync_store::reads::{ThreadSort, ThreadSummary};
use tokio_util::sync::CancellationToken;

use crate::error::EngineError;

/// A retained discussion paired with its best comparable chunk similarity.
///
/// Internal retrieval state; constructing this value does not establish vector eligibility.
#[derive(Clone, Debug)]
pub(crate) struct ScoredThread {
    /// Discussion identity, content, and coverage used to build the result.
    pub summary: ThreadSummary,
    /// Best chunk score; page scoring retains only finite positive values.
    pub score: f64,
}

/// Computes cosine similarity using scaled f64 accumulation to avoid f32 overflow.
///
/// Returns `None` for differing dimensions or zero magnitude. Otherwise returns a score clamped
/// to the inclusive range -1 through 1. Vector construction owns finite-value validation; this
/// calculation does not compare model identity or establish that the vectors share a model.
pub fn cosine_similarity(left: &EmbeddingVector, right: &EmbeddingVector) -> Option<f64> {
    if left.dimensions() != right.dimensions() {
        return None;
    }
    let left_values = left.values();
    let right_values = right.values();
    let mut left_scale = 0.0_f64;
    let mut right_scale = 0.0_f64;
    for (&left_value, &right_value) in left_values.iter().zip(right_values) {
        left_scale = left_scale.max(f64::from(left_value).abs());
        right_scale = right_scale.max(f64::from(right_value).abs());
    }
    if left_scale == 0.0 || right_scale == 0.0 {
        return None;
    }

    let mut dot = 0.0_f64;
    let mut left_norm = 0.0_f64;
    let mut right_norm = 0.0_f64;
    for (&left_value, &right_value) in left_values.iter().zip(right_values) {
        let left_value = f64::from(left_value) / left_scale;
        let right_value = f64::from(right_value) / right_scale;
        dot += left_value * right_value;
        left_norm += left_value * left_value;
        right_norm += right_value * right_value;
    }
    let score = dot / (left_norm.sqrt() * right_norm.sqrt());
    Some(score.clamp(-1.0, 1.0))
}

/// Scores one bounded page using each document's best chunk, skipping documents with any
/// dimension mismatch. Cancellation is checked between documents so a large scan can stop.
pub(crate) fn score_embedding_page(
    query: &EmbeddingVector,
    documents: Vec<EmbeddingSearchDocument>,
    sort: ThreadSort,
    limit: usize,
    cancellation: &CancellationToken,
) -> Result<Vec<ScoredThread>, EngineError> {
    let mut scored = Vec::with_capacity(documents.len().min(limit));
    for document in documents {
        if cancellation.is_cancelled() {
            return Err(EngineError::SearchCancelled);
        }
        if document
            .chunks
            .iter()
            .any(|chunk| chunk.vector.dimensions() != query.dimensions())
        {
            continue;
        }
        let score = document
            .chunks
            .iter()
            .filter_map(|chunk| cosine_similarity(query, &chunk.vector))
            .fold(f64::NEG_INFINITY, f64::max);
        if !score.is_finite() || score <= 0.0 {
            continue;
        }
        scored.push(ScoredThread {
            summary: document.summary,
            score,
        });
    }
    sort_scored(&mut scored, sort);
    scored.truncate(limit);
    Ok(scored)
}

/// Keeps only the best candidates after each page, bounding memory while preserving the final
/// sort order across the entire document scan.
pub(crate) fn merge_scored_pages(
    current: &mut Vec<ScoredThread>,
    page: Vec<ScoredThread>,
    sort: ThreadSort,
    limit: usize,
) {
    current.extend(page);
    sort_scored(current, sort);
    current.truncate(limit);
}

/// Orders candidates by the selected primary policy, then stable discussion identity.
pub(crate) fn sort_scored(scored: &mut [ScoredThread], sort: ThreadSort) {
    scored.sort_by(|left, right| compare_scored(left, right, sort));
}

/// Breaks equal primary scores or timestamps with deterministic archive discussion identity.
fn compare_scored(left: &ScoredThread, right: &ScoredThread, sort: ThreadSort) -> Ordering {
    let primary = match sort {
        ThreadSort::Relevance => right.score.total_cmp(&left.score),
        ThreadSort::Updated => right
            .summary
            .discussion
            .updated_at
            .cmp(&left.summary.discussion.updated_at),
        ThreadSort::Created => right
            .summary
            .discussion
            .created_at
            .cmp(&left.summary.discussion.created_at),
    };
    primary.then_with(|| stable_thread_id_cmp(&left.summary, &right.summary))
}

/// Compares archive discussion identities without depending on retrieval order.
pub(crate) fn stable_thread_id_cmp(left: &ThreadSummary, right: &ThreadSummary) -> Ordering {
    let left_id = &left.discussion.id;
    let right_id = &right.discussion.id;
    left_id
        .repository()
        .host()
        .cmp(right_id.repository().host())
        .then_with(|| {
            left_id
                .repository()
                .provider_id()
                .cmp(right_id.repository().provider_id())
        })
        .then_with(|| left_id.provider_id().cmp(right_id.provider_id()))
        .then_with(|| left_id.number().cmp(&right_id.number()))
}

#[cfg(test)]
#[path = "exact_search/tests.rs"]
mod tests;
