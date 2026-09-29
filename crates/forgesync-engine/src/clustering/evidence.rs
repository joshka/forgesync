//! # Select sparse pairwise evidence
//!
//! `CandidateEvidence` prepares references and title tokens once, scores eligible document pairs,
//! and retains bounded neighbors. Its single scoring rule serves both neighbor selection and final
//! edges. This keeps threshold, title support, and cross-kind safeguards consistent.
//!
//! `candidates` validates and orders input; `references` interprets explicit thread mentions;
//! `components` applies cluster-size policy after edges are selected. Cancellation is checked
//! during pair traversal. Selected edges are sorted deterministically before grouping.
//!
//! This is derived analysis over stored vectors, without archive writes or provider requests.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use forgesync_store::embeddings::EmbeddingSearchDocument;
use tokio_util::sync::CancellationToken;

use super::ClusterOptions;
use super::references::{
    MIN_TITLE_OVERLAP, deterministic_reference_edges, overlap_ratio, title_tokens,
};
use crate::error::EngineError;
use crate::exact_search::cosine_similarity;

/// Vector score above which same-kind edges need no additional title-token support.
/// Cross-kind edges still need the independently configured cross-kind threshold.
const HIGH_CONFIDENCE_SCORE: f64 = 0.90;

/// Pairwise evidence selected under one validated clustering policy.
///
/// References and title tokens are computed once. Both bounded neighbor selection and final edge
/// construction use `score`, so their eligibility rules cannot drift independently.
pub struct CandidateEvidence<'a> {
    /// Stable ordered vector/document snapshot shared by all pairwise indexes.
    documents: &'a [EmbeddingSearchDocument],
    /// Eligible explicit mention weights keyed by increasing source/target indexes.
    reference_edges: HashMap<(usize, usize), f64>,
    /// Precomputed title token sets in exactly the document snapshot order.
    title_overlaps: Vec<HashSet<String>>,
    /// Validated thresholds and neighbor bounds applied consistently by `score`.
    options: ClusterOptions,
}

impl<'a> CandidateEvidence<'a> {
    /// Prepares stable evidence indexes without changing the source documents.
    pub fn new(
        documents: &'a [EmbeddingSearchDocument],
        repository: &str,
        options: ClusterOptions,
    ) -> Self {
        Self {
            documents,
            reference_edges: deterministic_reference_edges(documents, repository),
            title_overlaps: documents
                .iter()
                .map(|document| title_tokens(&document.summary.discussion.title))
                .collect(),
            options,
        }
    }

    /// Selects bounded neighbors and orders their unique edges before component formation.
    pub fn edges(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CandidateEdge>, EngineError> {
        let neighbors = self.neighbors(cancellation)?;
        let mut selected =
            HashSet::with_capacity(self.documents.len().saturating_mul(self.options.fanout));
        for (left, list) in neighbors.iter().enumerate() {
            for neighbor in list {
                selected.insert((left.min(neighbor.node_index), left.max(neighbor.node_index)));
            }
        }
        let mut edges = selected
            .into_iter()
            .map(|(left, right)| CandidateEdge {
                left,
                right,
                score: self
                    .score(left, right)
                    .expect("selected edge has a candidate score"),
            })
            .collect::<Vec<_>>();
        edges.sort_by(compare_edges);
        Ok(edges)
    }

    /// Keeps the strongest eligible relationships per document with bounded memory.
    fn neighbors(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<Vec<BinaryHeap<Neighbor>>, EngineError> {
        let mut neighbors = (0..self.documents.len())
            .map(|_| BinaryHeap::with_capacity(self.options.fanout + 1))
            .collect::<Vec<_>>();
        for left in 0..self.documents.len() {
            if cancellation.is_cancelled() {
                return Err(EngineError::ClusteringCancelled);
            }
            for right in left + 1..self.documents.len() {
                if right % 1024 == 0 && cancellation.is_cancelled() {
                    return Err(EngineError::ClusteringCancelled);
                }
                let Some(score) = self.score(left, right) else {
                    continue;
                };
                offer_neighbor(
                    &mut neighbors[left],
                    Neighbor {
                        node_index: right,
                        score,
                    },
                    self.options.fanout,
                );
                offer_neighbor(
                    &mut neighbors[right],
                    Neighbor {
                        node_index: left,
                        score,
                    },
                    self.options.fanout,
                );
            }
        }
        Ok(neighbors)
    }

    /// Combines explicit references with similarity that satisfies title and kind safeguards.
    fn score(&self, left: usize, right: usize) -> Option<f64> {
        let similarity =
            document_similarity(&self.documents[left], &self.documents[right]).filter(|score| {
                *score >= self.options.threshold
                    && (*score >= HIGH_CONFIDENCE_SCORE
                        || overlap_ratio(&self.title_overlaps[left], &self.title_overlaps[right])
                            >= MIN_TITLE_OVERLAP)
                    && (self.documents[left].summary.discussion.kind
                        == self.documents[right].summary.discussion.kind
                        || *score >= self.options.cross_kind_threshold)
            });
        max_score(
            similarity,
            self.reference_edges.get(&(left, right)).copied(),
        )
    }
}

/// Keeps only the strongest bounded neighbors for one discussion.
fn offer_neighbor(heap: &mut BinaryHeap<Neighbor>, candidate: Neighbor, capacity: usize) {
    if heap.len() < capacity {
        heap.push(candidate);
        return;
    }
    let Some(worst) = heap.peek() else {
        return;
    };
    let is_better = candidate.score > worst.score
        || (candidate.score.total_cmp(&worst.score) == Ordering::Equal
            && candidate.node_index < worst.node_index);
    if is_better {
        heap.pop();
        heap.push(candidate);
    }
}

/// Scores compatible discussion vectors for a candidate edge.
fn document_similarity(
    left: &EmbeddingSearchDocument,
    right: &EmbeddingSearchDocument,
) -> Option<f64> {
    left.chunks
        .iter()
        .flat_map(|left_chunk| {
            right.chunks.iter().filter_map(move |right_chunk| {
                cosine_similarity(&left_chunk.vector, &right_chunk.vector)
            })
        })
        .filter(|score| score.is_finite())
        .max_by(f64::total_cmp)
}

/// Orders edges deterministically before component construction.
fn compare_edges(left: &CandidateEdge, right: &CandidateEdge) -> Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.left.cmp(&right.left))
        .then_with(|| left.right.cmp(&right.right))
}

/// Returns the strongest similarity supporting a component edge.
fn max_score(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(score), None) | (None, Some(score)) => Some(score),
        (None, None) => None,
    }
}

/// One retained neighbor in a bounded max-heap whose head is the worst retained candidate.
///
/// Reverse score ordering permits constant-time rejection of weaker incoming candidates. Equal
/// scores retain the lower stable document index; indexes belong to this build's snapshot only.
#[derive(Clone, Copy, Debug)]
pub struct Neighbor {
    /// Target index in the stable candidate document snapshot.
    pub node_index: usize,
    /// Finite ranking weight from eligible vector or explicit-reference evidence.
    pub score: f64,
}

impl PartialEq for Neighbor {
    /// Compares node identity and total-order score, keeping equality consistent with heap
    /// ordering.
    fn eq(&self, other: &Self) -> bool {
        self.node_index == other.node_index && self.score.total_cmp(&other.score) == Ordering::Equal
    }
}

impl Eq for Neighbor {}

impl PartialOrd for Neighbor {
    /// Uses the total heap order even for floating-point scores, avoiding unordered comparisons.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Neighbor {
    /// Places the worst retained neighbor at the max-heap head by reversing score order.
    /// Node-index ties are deterministic, allowing pruning to retain the lower-index neighbor.
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .score
            .total_cmp(&self.score)
            .then_with(|| self.node_index.cmp(&other.node_index))
    }
}

/// Eligible undirected relationship proposed to bounded component construction.
///
/// Endpoints are increasing snapshot indexes. The score is the strongest eligible vector or
/// reference evidence, so it must not be interpreted as a probability or exclusively as cosine.
#[derive(Clone, Copy, Debug)]
pub struct CandidateEdge {
    /// Lower endpoint index in the candidate document snapshot.
    pub left: usize,
    /// Higher endpoint index in the candidate document snapshot.
    pub right: usize,
    /// Finite ranking weight from eligible vector or explicit-reference evidence.
    pub score: f64,
}
