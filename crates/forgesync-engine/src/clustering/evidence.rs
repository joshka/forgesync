//! Select sparse pairwise evidence.
//!
//! One scoring rule serves neighbor selection, and selected edges reuse the scores computed then,
//! so threshold, title support, and cross-kind safeguards cannot drift between phases. Chunk
//! vectors are normalized once, making each pairwise cosine a dot product.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use forgesync_store::embeddings::EmbeddingSearchDocument;
use tokio_util::sync::CancellationToken;

use super::ClusterOptions;
use super::references::{
    MIN_TITLE_OVERLAP, deterministic_reference_edges, overlap_ratio, title_tokens,
};
use crate::error::EngineError;

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
    /// Unit-length chunk vectors per document; zero-magnitude chunks are omitted.
    unit_chunks: Vec<Vec<Vec<f64>>>,
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
            unit_chunks: documents
                .iter()
                .map(|document| {
                    document
                        .chunks
                        .iter()
                        .filter_map(|chunk| unit_vector(chunk.vector.values()))
                        .collect()
                })
                .collect(),
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
            HashMap::with_capacity(self.documents.len().saturating_mul(self.options.fanout));
        for (left, list) in neighbors.iter().enumerate() {
            for neighbor in list {
                let key = (left.min(neighbor.node_index), left.max(neighbor.node_index));
                selected.insert(key, neighbor.score);
            }
        }
        let mut edges = selected
            .into_iter()
            .map(|((left, right), score)| CandidateEdge { left, right, score })
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
            max_dot(&self.unit_chunks[left], &self.unit_chunks[right]).filter(|score| {
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

/// Scales a vector to unit length in f64, or `None` for zero magnitude.
fn unit_vector(values: &[f32]) -> Option<Vec<f64>> {
    let norm = values
        .iter()
        .map(|&value| f64::from(value) * f64::from(value))
        .sum::<f64>()
        .sqrt();
    (norm > 0.0).then(|| {
        values
            .iter()
            .map(|&value| f64::from(value) / norm)
            .collect()
    })
}

/// Best cosine between any two equal-dimension unit chunks of two discussions.
fn max_dot(left: &[Vec<f64>], right: &[Vec<f64>]) -> Option<f64> {
    left.iter()
        .flat_map(|left| {
            right
                .iter()
                .filter(|right| right.len() == left.len())
                .map(move |right| {
                    let dot: f64 = left.iter().zip(right).map(|(l, r)| l * r).sum();
                    dot.clamp(-1.0, 1.0)
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
