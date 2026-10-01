//! Ranked discussion candidates and their shared ordering.
//!
//! Relevance ordering uses score, while explicit created/updated ordering uses source timestamps.
//! Stable discussion identity breaks all primary ties, so search and fusion output never depends on
//! retrieval or hash-map order.

use std::cmp::Ordering;

use forgesync_core::embedding::EmbeddingVector;
use forgesync_store::embeddings::EmbeddingSearchDocument;
use forgesync_store::reads::{ThreadSort, ThreadSummary};
use tokio_util::sync::CancellationToken;

use crate::error::EngineError;
use crate::exact_search::cosine_similarity;

/// A retained discussion paired with its best comparable chunk similarity.
#[derive(Clone, Debug)]
pub struct ScoredThread {
    pub summary: ThreadSummary,
    /// Best chunk score; scoring retains only finite positive values.
    pub score: f64,
}

/// Bounded best-first ranking of documents against one query vector.
pub struct TopScored {
    query: EmbeddingVector,
    sort: ThreadSort,
    limit: usize,
    ranked: Vec<ScoredThread>,
    /// Documents whose every chunk has the query dimension, independent of the result bound.
    compatible: usize,
}

impl TopScored {
    /// Starts an empty ranking that keeps at most `limit` winners under `sort`.
    pub fn new(query: EmbeddingVector, sort: ThreadSort, limit: usize) -> Self {
        Self {
            query,
            sort,
            limit,
            ranked: Vec::new(),
            compatible: 0,
        }
    }

    /// Scores each document by its best chunk, skipping documents with any dimension mismatch.
    ///
    /// Candidates are pruned to the bound only when twice the bound accumulates, so the whole
    /// prefix is not re-sorted for every page.
    pub fn add(
        &mut self,
        documents: Vec<EmbeddingSearchDocument>,
        cancellation: &CancellationToken,
    ) -> Result<(), EngineError> {
        for document in documents {
            if cancellation.is_cancelled() {
                return Err(EngineError::SearchCancelled);
            }
            let dimensions = self.query.dimensions();
            if document
                .chunks
                .iter()
                .any(|chunk| chunk.vector.dimensions() != dimensions)
            {
                continue;
            }
            self.compatible += 1;
            let score = document
                .chunks
                .iter()
                .filter_map(|chunk| cosine_similarity(&self.query, &chunk.vector))
                .fold(f64::NEG_INFINITY, f64::max);
            if score.is_finite() && score > 0.0 {
                self.ranked.push(ScoredThread {
                    summary: document.summary,
                    score,
                });
            }
            if self.ranked.len() >= self.limit.saturating_mul(2).max(1) {
                self.prune();
            }
        }
        Ok(())
    }

    /// Returns the ordered winners and the number of dimension-compatible documents seen.
    pub fn finish(mut self) -> (Vec<ScoredThread>, usize) {
        self.prune();
        (self.ranked, self.compatible)
    }

    /// Orders the retained candidates and drops those beyond the bound.
    fn prune(&mut self) {
        let sort = self.sort;
        self.ranked.sort_by(|left, right| {
            compare_ranked(sort, &left.summary, left.score, &right.summary, right.score)
        });
        self.ranked.truncate(self.limit);
    }
}

/// Orders two scored discussions by the requested primary policy, then stable identity.
pub fn compare_ranked(
    sort: ThreadSort,
    left: &ThreadSummary,
    left_score: f64,
    right: &ThreadSummary,
    right_score: f64,
) -> Ordering {
    let primary = match sort {
        ThreadSort::Relevance => right_score.total_cmp(&left_score),
        ThreadSort::Updated => right.discussion.updated_at.cmp(&left.discussion.updated_at),
        ThreadSort::Created => right.discussion.created_at.cmp(&left.discussion.created_at),
    };
    primary.then_with(|| stable_thread_id_cmp(left, right))
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
