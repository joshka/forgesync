//! # Project bounded components into cluster proposals
//!
//! `format_clusters` receives stable document indexes and edges retained by `components`. It
//! constructs `ClusterCandidate` and `ClusterMemberCandidate` values for the generation workflow.
//! These values describe automatic analysis; the store later reconciles local maintainer decisions.
//!
//! `ClusterProjection` prepares retained degrees and direct edge weights once for all components.
//! Its methods name the representative ranking and member-score rules instead of leaving those
//! policies inside a large iterator closure. The representative has highest retained degree, then
//! the lowest discussion number and stable identity. A transitive member without a direct edge to
//! that representative has no score; this is distinct from a zero similarity score.
//!
//! Input indexes refer to one immutable document snapshot. This module neither acquires vectors nor
//! modifies the archive, and it does not compute new pairwise similarities while projecting groups.

use std::cmp::Ordering;
use std::collections::HashMap;

use forgesync_core::identity::ThreadId;
use forgesync_store::embeddings::EmbeddingSearchDocument;
use forgesync_store::reads::ThreadSummary;

use super::evidence::CandidateEdge;
use crate::scoring::stable_thread_id_cmp;

/// Derived member projection awaiting reconciliation with durable local maintainer decisions.
#[derive(Clone, Debug, PartialEq)]
pub struct ClusterMemberCandidate {
    /// Archived discussion identity and display content selected for this generation.
    pub summary: ThreadSummary,
    /// Direct retained edge weight to the representative, or none for transitive-only membership.
    /// The representative receives `1.0`; other weights may come from vector or reference
    /// evidence.
    pub score_to_representative: Option<f64>,
}

/// Bounded graph component proposed for one derived cluster generation.
///
/// Representative selection uses retained degree and deterministic identity ties. It is an
/// analysis proposal: the store owns durable cluster identity and reconciles canonical-member and
/// inclusion decisions before presenting the resulting cluster.
#[derive(Clone, Debug, PartialEq)]
pub struct ClusterCandidate {
    /// Automatically selected member identity, before applying local canonical choices.
    pub representative: ThreadId,
    /// Archived title copied from the automatically selected representative.
    pub title: String,
    /// Stable identity-ordered members that satisfy component size limits.
    pub members: Vec<ClusterMemberCandidate>,
}

/// Converts eligible graph components into stable automatic generation proposals.
///
/// Component and edge indexes must refer to `documents`; component members are already in stable
/// identity order. `min_size` comes from validated options and is positive, so empty components are
/// filtered before representative selection. Only retained edges contribute degree or scores.
pub fn format_clusters(
    documents: &[EmbeddingSearchDocument],
    components: &[Vec<usize>],
    edges: &[CandidateEdge],
    min_size: usize,
) -> Vec<ClusterCandidate> {
    let projection = ClusterProjection::new(documents, edges);
    components
        .iter()
        .filter(|members| members.len() >= min_size)
        .map(|members| projection.cluster(members))
        .collect()
}

/// Immutable document snapshot and retained-edge indexes shared by all proposed groups.
struct ClusterProjection<'a> {
    /// Stable candidate documents indexed by each component member and edge endpoint.
    documents: &'a [EmbeddingSearchDocument],
    /// Retained incident-edge counts used to rank representatives.
    degrees: Vec<usize>,
    /// Direct relationship weights keyed by increasing snapshot endpoint indexes.
    edge_scores: HashMap<(usize, usize), f64>,
}

impl<'a> ClusterProjection<'a> {
    /// Prepares degree and direct-score indexes from retained edges without rescoring pairs.
    fn new(documents: &'a [EmbeddingSearchDocument], edges: &[CandidateEdge]) -> Self {
        let mut degrees = vec![0_usize; documents.len()];
        let mut edge_scores = HashMap::with_capacity(edges.len());
        for edge in edges {
            degrees[edge.left] = degrees[edge.left].saturating_add(1);
            degrees[edge.right] = degrees[edge.right].saturating_add(1);
            edge_scores.insert(
                (edge.left.min(edge.right), edge.left.max(edge.right)),
                edge.score,
            );
        }
        Self {
            documents,
            degrees,
            edge_scores,
        }
    }

    /// Projects one nonempty component, preserving its already stable member order.
    fn cluster(&self, members: &[usize]) -> ClusterCandidate {
        let representative = self.representative(members);
        let discussion = &self.documents[representative].summary.discussion;
        let members = members
            .iter()
            .map(|member| self.member(*member, representative))
            .collect();
        ClusterCandidate {
            representative: discussion.id.clone(),
            title: discussion.title.clone(),
            members,
        }
    }

    /// Selects the highest-degree member, breaking ties by discussion number and stable identity.
    fn representative(&self, members: &[usize]) -> usize {
        members
            .iter()
            .copied()
            .min_by(|left, right| self.compare_representatives(*left, *right))
            .expect("a component has at least one member")
    }

    /// Orders stronger representatives first without depending on component traversal order.
    fn compare_representatives(&self, left: usize, right: usize) -> Ordering {
        let left_summary = &self.documents[left].summary;
        let right_summary = &self.documents[right].summary;
        let left_number = left_summary.discussion.id.number();
        let right_number = right_summary.discussion.id.number();
        self.degrees[right]
            .cmp(&self.degrees[left])
            .then_with(|| left_number.cmp(&right_number))
            .then_with(|| stable_thread_id_cmp(left_summary, right_summary))
    }

    /// Copies the archived member and attaches only its direct retained representative edge.
    fn member(&self, member: usize, representative: usize) -> ClusterMemberCandidate {
        let score_to_representative = if member == representative {
            Some(1.0)
        } else {
            let pair = (member.min(representative), member.max(representative));
            self.edge_scores.get(&pair).copied()
        };
        ClusterMemberCandidate {
            summary: self.documents[member].summary.clone(),
            score_to_representative,
        }
    }
}

#[cfg(test)]
#[path = "proposal_tests.rs"]
mod tests;
