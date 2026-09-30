//! # Turn selected edges into bounded components
//!
//! `bounded_components` applies cluster-size limits to deterministic edges using union-find.
//! `proposals` chooses representatives and constructs member projections after grouping. A rejected
//! union cannot grow a component beyond the configured maximum.
//!
//! `evidence` decides which edges qualify; this module decides how qualifying edges form groups.
//! The input document order and edge order are already stable. Representatives use retained degree
//! and deterministic identity ties in that neighboring module. The store reconciles proposals with
//! local decisions.
//!
//! These pure transformations never modify archive state or source observations.

use std::collections::HashMap;

use forgesync_store::embeddings::EmbeddingSearchDocument;

use super::ClusterOptions;
use super::evidence::CandidateEdge;
use crate::scoring::stable_thread_id_cmp;

/// Builds connected groups without exceeding the configured size.
pub fn bounded_components(
    documents: &[EmbeddingSearchDocument],
    edges: &[CandidateEdge],
    options: ClusterOptions,
) -> (Vec<Vec<usize>>, Vec<CandidateEdge>) {
    let mut groups = UnionFind::new(documents.len());
    let mut kept_edges = Vec::with_capacity(edges.len());
    for edge in edges {
        if groups.union(edge.left, edge.right, options.max_cluster_size) {
            kept_edges.push(*edge);
        }
    }
    let mut components = HashMap::<usize, Vec<usize>>::new();
    for index in 0..documents.len() {
        let root = groups.find(index);
        components.entry(root).or_default().push(index);
    }
    let mut components = components.into_values().collect::<Vec<_>>();
    for members in &mut components {
        members.sort_by(|left, right| {
            stable_thread_id_cmp(&documents[*left].summary, &documents[*right].summary)
        });
    }
    components.sort_by(|left, right| {
        right.len().cmp(&left.len()).then_with(|| {
            stable_thread_id_cmp(&documents[left[0]].summary, &documents[right[0]].summary)
        })
    });
    (components, kept_edges)
}

/// Disjoint-set forest that refuses unions exceeding the configured component size.
///
/// Each candidate starts as a singleton. Only root sizes are authoritative; path compression
/// shortens later lookups while union-by-size bounds tree depth.
struct UnionFind {
    /// Parent indexes into the same forest; a root points to itself.
    parent: Vec<usize>,
    /// Current component sizes at root indexes; non-root entries are not consulted.
    size: Vec<usize>,
}

impl UnionFind {
    /// Creates one disjoint-set entry per candidate discussion.
    fn new(count: usize) -> Self {
        Self {
            parent: (0..count).collect(),
            size: vec![1; count],
        }
    }

    /// Finds a component root while shortening its parent path.
    fn find(&mut self, index: usize) -> usize {
        if self.parent[index] != index {
            self.parent[index] = self.find(self.parent[index]);
        }
        self.parent[index]
    }

    /// Joins eligible components when the resulting group stays bounded.
    fn union(&mut self, left: usize, right: usize, max_size: usize) -> bool {
        let mut left_root = self.find(left);
        let mut right_root = self.find(right);
        if left_root == right_root {
            return true;
        }
        if self.size[left_root] < self.size[right_root] {
            std::mem::swap(&mut left_root, &mut right_root);
        }
        if self.size[left_root].saturating_add(self.size[right_root]) > max_size {
            return false;
        }
        self.parent[right_root] = left_root;
        self.size[left_root] += self.size[right_root];
        true
    }
}
