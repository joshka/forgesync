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

use super::references::{deterministic_reference_edges, overlap_ratio, title_tokens};
use super::{
    BinaryHeap, CancellationToken, CandidateEdge, ClusterOptions, EmbeddingSearchDocument,
    EngineError, HIGH_CONFIDENCE_SCORE, HashMap, HashSet, MIN_TITLE_OVERLAP, Neighbor, Ordering,
    cosine_similarity,
};

/// Pairwise evidence selected under one validated clustering policy.
///
/// References and title tokens are computed once. Both bounded neighbor selection and final edge
/// construction use `score`, so their eligibility rules cannot drift independently.
pub struct CandidateEvidence<'a> {
    documents: &'a [EmbeddingSearchDocument],
    reference_edges: HashMap<(usize, usize), f64>,
    title_overlaps: Vec<HashSet<String>>,
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
