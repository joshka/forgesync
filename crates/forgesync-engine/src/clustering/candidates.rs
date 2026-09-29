//! Candidates cluster behavior.

use super::{
    BinaryHeap, CancellationToken, CandidateEdge, ClusterCandidate, ClusterMemberCandidate,
    ClusterOptions, EARLY_BODY_REFERENCE_BYTES, EmbeddingSearchDocument, EngineError,
    HIGH_CONFIDENCE_SCORE, HashMap, HashSet, MIN_TITLE_OVERLAP, Neighbor, Ordering,
    REFERENCE_SCORE, THREAD_REFERENCE, TITLE_TOKEN, cosine_similarity, stable_thread_id_cmp,
};

/// Builds sparse candidate components from current discussion vectors and references.
pub(crate) fn build_cluster_candidates(
    mut documents: Vec<EmbeddingSearchDocument>,
    repository_full_name: &str,
    options: ClusterOptions,
    cancellation: &CancellationToken,
) -> Result<(Vec<ClusterCandidate>, usize), EngineError> {
    let options = options.validate()?;
    documents.sort_by(|left, right| stable_thread_id_cmp(&left.summary, &right.summary));
    if cancellation.is_cancelled() {
        return Err(EngineError::ClusteringCancelled);
    }

    let thread_index = documents
        .iter()
        .enumerate()
        .map(|(index, document)| (document.summary.discussion.id.clone(), index))
        .collect::<HashMap<_, _>>();
    if thread_index.len() != documents.len() {
        return Err(EngineError::InvalidClusterInput);
    }
    let reference_edges = deterministic_reference_edges(&documents, repository_full_name);
    let title_overlaps = documents
        .iter()
        .map(|document| title_tokens(&document.summary.discussion.title))
        .collect::<Vec<_>>();
    let mut neighbors = (0..documents.len())
        .map(|_| BinaryHeap::with_capacity(options.fanout + 1))
        .collect::<Vec<_>>();

    for left in 0..documents.len() {
        if cancellation.is_cancelled() {
            return Err(EngineError::ClusteringCancelled);
        }
        for right in left + 1..documents.len() {
            if right % 1024 == 0 && cancellation.is_cancelled() {
                return Err(EngineError::ClusteringCancelled);
            }
            let left_discussion = &documents[left].summary.discussion;
            let right_discussion = &documents[right].summary.discussion;
            let similarity = document_similarity(&documents[left], &documents[right]);
            let similarity_is_candidate = similarity.is_some_and(|score| {
                score >= options.threshold
                    && (score >= HIGH_CONFIDENCE_SCORE
                        || overlap_ratio(&title_overlaps[left], &title_overlaps[right])
                            >= MIN_TITLE_OVERLAP)
                    && (left_discussion.kind == right_discussion.kind
                        || score >= options.cross_kind_threshold)
            });
            let reference_score = reference_edges.get(&(left, right)).copied();
            let Some(score) = (if similarity_is_candidate {
                max_score(similarity, reference_score)
            } else {
                reference_score
            }) else {
                continue;
            };
            offer_neighbor(
                &mut neighbors[left],
                Neighbor {
                    node_index: right,
                    score,
                },
                options.fanout,
            );
            offer_neighbor(
                &mut neighbors[right],
                Neighbor {
                    node_index: left,
                    score,
                },
                options.fanout,
            );
        }
    }

    let mut selected = HashSet::with_capacity(documents.len().saturating_mul(options.fanout));
    for (left, list) in neighbors.iter().enumerate() {
        for neighbor in list {
            selected.insert((left.min(neighbor.node_index), left.max(neighbor.node_index)));
        }
    }
    let mut edges = selected
        .into_iter()
        .map(|(left, right)| {
            let similarity = document_similarity(&documents[left], &documents[right]);
            let reference = reference_edges.get(&(left, right)).copied();
            let score = max_score(
                similarity.filter(|score| {
                    *score >= options.threshold
                        && (*score >= HIGH_CONFIDENCE_SCORE
                            || overlap_ratio(&title_overlaps[left], &title_overlaps[right])
                                >= MIN_TITLE_OVERLAP)
                        && (documents[left].summary.discussion.kind
                            == documents[right].summary.discussion.kind
                            || *score >= options.cross_kind_threshold)
                }),
                reference,
            )
            .expect("selected edge has a candidate score");
            CandidateEdge { left, right, score }
        })
        .collect::<Vec<_>>();
    edges.sort_by(compare_edges);
    let edge_count = edges.len();
    let (clusters, kept_edges) = bounded_components(&documents, &edges, options);
    let candidates = format_clusters(&documents, &clusters, &kept_edges, options.min_cluster_size);
    Ok((candidates, edge_count))
}

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

fn deterministic_reference_edges(
    documents: &[EmbeddingSearchDocument],
    repository_full_name: &str,
) -> HashMap<(usize, usize), f64> {
    let by_number = documents
        .iter()
        .enumerate()
        .map(|(index, document)| (document.summary.discussion.id.number().get(), index))
        .collect::<HashMap<_, _>>();
    let titles = documents
        .iter()
        .map(|document| title_tokens(&document.summary.discussion.title))
        .collect::<Vec<_>>();
    let mut edges = HashMap::new();
    for (source_index, document) in documents.iter().enumerate() {
        let discussion = &document.summary.discussion;
        collect_reference_edges(
            &mut edges,
            source_index,
            &discussion.title,
            true,
            repository_full_name,
            discussion.id.number().get(),
            &by_number,
            &titles,
        );
        if let Some(body) = discussion.body.as_deref() {
            collect_reference_edges(
                &mut edges,
                source_index,
                body,
                false,
                repository_full_name,
                discussion.id.number().get(),
                &by_number,
                &titles,
            );
        }
    }
    edges
}

#[allow(clippy::too_many_arguments)]
fn collect_reference_edges(
    edges: &mut HashMap<(usize, usize), f64>,
    source_index: usize,
    text: &str,
    is_title: bool,
    repository_full_name: &str,
    source_number: u64,
    by_number: &HashMap<u64, usize>,
    titles: &[HashSet<String>],
) {
    for captures in THREAD_REFERENCE.captures_iter(text) {
        let referenced_repository = captures.get(1).or_else(|| captures.get(3));
        if referenced_repository.is_some_and(|repository| {
            !repository
                .as_str()
                .eq_ignore_ascii_case(repository_full_name)
        }) {
            continue;
        }
        let number_capture = captures
            .get(2)
            .or_else(|| captures.get(4))
            .or_else(|| captures.get(5));
        let Some(number) = number_capture.and_then(|value| value.as_str().parse::<u64>().ok())
        else {
            continue;
        };
        if number == source_number {
            continue;
        }
        let Some(&target_index) = by_number.get(&number) else {
            continue;
        };
        let early_body = !is_title
            && captures
                .get(0)
                .is_some_and(|reference| reference.start() <= EARLY_BODY_REFERENCE_BYTES);
        if !is_title
            && !early_body
            && overlap_ratio(&titles[source_index], &titles[target_index]) < MIN_TITLE_OVERLAP
        {
            continue;
        }
        edges
            .entry((
                source_index.min(target_index),
                source_index.max(target_index),
            ))
            .and_modify(|score| *score = score.max(REFERENCE_SCORE))
            .or_insert(REFERENCE_SCORE);
    }
}

fn title_tokens(value: &str) -> HashSet<String> {
    TITLE_TOKEN
        .find_iter(value)
        .map(|token| token.as_str().to_ascii_lowercase())
        .collect()
}

fn overlap_ratio(left: &HashSet<String>, right: &HashSet<String>) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let overlap = left.intersection(right).count();
    overlap as f64 / left.len().min(right.len()) as f64
}

fn compare_edges(left: &CandidateEdge, right: &CandidateEdge) -> Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.left.cmp(&right.left))
        .then_with(|| left.right.cmp(&right.right))
}

fn bounded_components(
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

fn format_clusters(
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
    fn new(count: usize) -> Self {
        Self {
            parent: (0..count).collect(),
            size: vec![1; count],
        }
    }

    fn find(&mut self, index: usize) -> usize {
        if self.parent[index] != index {
            self.parent[index] = self.find(self.parent[index]);
        }
        self.parent[index]
    }

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

fn max_score(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(score), None) | (None, Some(score)) => Some(score),
        (None, None) => None,
    }
}
