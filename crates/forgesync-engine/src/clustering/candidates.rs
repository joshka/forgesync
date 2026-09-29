//! # Form duplicate candidates from thread relationships
//!
//! Candidate building uses available similarity evidence and a union-find grouping step to turn
//! pairwise relationships into cluster proposals. A proposed group is derived analysis, not a
//! source observation or a maintainer decision.
//!
//! The builder returns candidate structures for `build` to persist. Keeping grouping here lets a
//! reader inspect threshold and transitive-membership behavior without following database
//! transactions or CLI rendering.

use std::collections::HashMap;

use forgesync_store::embeddings::EmbeddingSearchDocument;
use tokio_util::sync::CancellationToken;

use super::ClusterOptions;
use super::components::bounded_components;
use super::evidence::CandidateEvidence;
use super::proposals::{ClusterCandidate, format_clusters};
use crate::error::EngineError;
use crate::exact_search::stable_thread_id_cmp;

/// Builds sparse candidate components from current discussion vectors and references.
pub fn build_cluster_candidates(
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
    let evidence = CandidateEvidence::new(&documents, repository_full_name, options);
    let edges = evidence.edges(cancellation)?;
    let edge_count = edges.len();
    let (clusters, kept_edges) = bounded_components(&documents, &edges, options);
    let candidates = format_clusters(&documents, &clusters, &kept_edges, options.min_cluster_size);
    Ok((candidates, edge_count))
}
