//! # Internal ranked discussion candidates
//!
//! [`ScoredThread`] pairs a retained projection with its best comparable chunk score. Page scoring
//! rejects documents with dimension-mismatched chunks and retains finite positive maxima. The
//! public cosine arithmetic lives in `exact_search`; this module adds retrieval policy around it.
//!
//! Relevance ordering uses score, while explicit created/updated ordering uses source timestamps.
//! Stable discussion identity breaks all primary ties. Page merging keeps a bounded prefix under
//! the same policy without deduplicating repeated identities. Callers must use consistent policy
//! and ensure vector model, recipe, freshness, and source eligibility before scoring.
//!
//! Search and clustering share identity ordering, but this module reads no archive and performs
//! no provider requests. Cancellation is checked between documents. Ordinary public visibility is
//! constrained by the private module boundary rather than exported as a public search API.

use std::cmp::Ordering;

use forgesync_core::embedding::EmbeddingVector;
use forgesync_store::embeddings::EmbeddingSearchDocument;
use forgesync_store::reads::{ThreadSort, ThreadSummary};
use tokio_util::sync::CancellationToken;

use crate::error::EngineError;
use crate::exact_search::cosine_similarity;

/// A retained discussion paired with its best comparable chunk similarity.
///
/// Internal retrieval state; constructing this value does not establish vector eligibility.
#[derive(Clone, Debug)]
pub struct ScoredThread {
    /// Discussion identity, content, and coverage used to build the result.
    pub summary: ThreadSummary,
    /// Best chunk score; page scoring retains only finite positive values.
    pub score: f64,
}

/// Scores one bounded page using each document's best chunk, skipping documents with any
/// dimension mismatch. Cancellation is checked between documents so a large scan can stop.
pub fn score_embedding_page(
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
pub fn merge_scored_pages(
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
pub fn sort_scored(scored: &mut [ScoredThread], sort: ThreadSort) {
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
pub fn stable_thread_id_cmp(left: &ThreadSummary, right: &ThreadSummary) -> Ordering {
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
#[path = "scoring/tests.rs"]
mod tests;
