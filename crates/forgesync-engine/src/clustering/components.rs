//! # Turn selected edges into bounded components
//!
//! `bounded_components` applies cluster-size limits to deterministic edges using union-find.
//! `format_clusters` chooses a representative and constructs stable member projections. A rejected
//! union cannot grow a component beyond the configured maximum.
//!
//! `evidence` decides which edges qualify; this module decides how qualifying edges form groups.
//! The input document order and edge order are already stable. Representatives use retained degree
//! and deterministic identity ties. The store later reconciles these proposals with local
//! decisions.
//!
//! These pure transformations never modify archive state or source observations.

use super::{
    CandidateEdge, ClusterCandidate, ClusterMemberCandidate, ClusterOptions,
    EmbeddingSearchDocument, HashMap, stable_thread_id_cmp,
};

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

/// Converts graph components into stable generation inputs.
pub fn format_clusters(
    documents: &[EmbeddingSearchDocument],
    components: &[Vec<usize>],
    edges: &[CandidateEdge],
    min_size: usize,
) -> Vec<ClusterCandidate> {
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
    components
        .iter()
        .filter(|members| members.len() >= min_size)
        .map(|members| {
            let representative = members
                .iter()
                .copied()
                .min_by(|left, right| {
                    degrees[*right]
                        .cmp(&degrees[*left])
                        .then_with(|| {
                            documents[*left]
                                .summary
                                .discussion
                                .id
                                .number()
                                .cmp(&documents[*right].summary.discussion.id.number())
                        })
                        .then_with(|| {
                            stable_thread_id_cmp(
                                &documents[*left].summary,
                                &documents[*right].summary,
                            )
                        })
                })
                .expect("a component has at least one member");
            let representative_id = documents[representative].summary.discussion.id.clone();
            let title = documents[representative].summary.discussion.title.clone();
            let members = members
                .iter()
                .map(|member| ClusterMemberCandidate {
                    summary: documents[*member].summary.clone(),
                    score_to_representative: if *member == representative {
                        Some(1.0)
                    } else {
                        edge_scores
                            .get(&((*member).min(representative), (*member).max(representative)))
                            .copied()
                    },
                })
                .collect();
            ClusterCandidate {
                representative: representative_id,
                title,
                members,
            }
        })
        .collect()
}

struct UnionFind {
    parent: Vec<usize>,
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
