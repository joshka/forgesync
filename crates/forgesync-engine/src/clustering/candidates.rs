//! # Turn pairwise evidence into bounded cluster proposals
//!
//! [`build_cluster_candidates`] coordinates pure derived analysis over supplied discussion/vector
//! projections. It validates graph options, sorts inputs by stable discussion identity, rejects
//! duplicate identities, and checks cancellation before constructing evidence indexes. It does
//! not query an archive or verify model, recipe, source freshness, or repository membership.
//!
//! The evidence module selects sparse pairwise edges using vector, title, and explicit-reference
//! support. The components module groups those edges under the component-size policy. The proposals
//! module formats retained groups, applying the minimum-size rule and recording their evidence.
//! Keeping these owners separate exposes the progression from source projections to relationships
//! to groups without mixing persistence or maintainer decisions into the graph calculation.
//!
//! The returned count is the selected edge count before bounded grouping, not necessarily the
//! number of edges represented in persisted candidates. The build workflow owns vector eligibility,
//! archive lease coordination, generation persistence, and coverage reporting. A candidate remains
//! a proposal rather than a source observation or a human decision.

use std::collections::HashMap;

use forgesync_store::embeddings::EmbeddingSearchDocument;
use tokio_util::sync::CancellationToken;

use crate::clustering::ClusterOptions;
use crate::clustering::components::bounded_components;
use crate::clustering::evidence::CandidateEvidence;
use crate::clustering::proposals::{ClusterCandidate, format_clusters};
use crate::error::EngineError;
use crate::scoring::stable_thread_id_cmp;

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
