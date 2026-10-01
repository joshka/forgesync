//! # Duplicate-cluster analysis and triage requests
//!
//! `ClusterOptions` tunes candidate generation. `ClusterBuildRequest` selects the archive scope
//! and `ClusterBuildReport` reports what was generated. List and detail requests support
//! inspection of stored clusters.
//!
//! `candidates` validates stable input, `evidence` selects eligible relationships, `references`
//! interprets explicit mentions, `components` applies bounded grouping, and `proposals` owns
//! representative policy. `build` commits a derived generation, `decisions`
//! applies local maintainer choices, and `lease` keeps competing operations from writing the same
//! analysis concurrently. The store owns durable generations and decision events; this module owns
//! analysis policy and workflow boundaries. Cluster actions affect the local archive only.

use forgesync_core::document::DocumentRecipe;
use forgesync_store::clusters::ClusterGenerationResult;
use serde::Serialize;

use crate::error::EngineError;
use crate::reference::RepositorySelector;

/// Baseline cosine threshold for same-kind candidate edges; supporting title policy still applies.
const DEFAULT_CLUSTER_THRESHOLD: f64 = 0.80;
/// Stricter cosine threshold for issue/pull-request edges, where similarity can reflect related
/// work.
const DEFAULT_CROSS_KIND_THRESHOLD: f64 = 0.93;

/// Tuning options for deterministic related-discussion clustering.
///
/// Similarity thresholds govern vector evidence. Explicit repository-scoped references can provide
/// separate evidence; the resulting graph is then pruned by fanout and bounded component size.
/// These are analysis heuristics, not probabilities that two discussions are duplicates.
///
/// Validate a customized policy before building a request:
///
/// ```
/// use forgesync_engine::clustering::ClusterOptions;
///
/// let options = ClusterOptions {
///     max_cluster_size: 20,
///     ..Default::default()
/// };
/// let options = options.validate()?;
/// assert_eq!(options.max_cluster_size, 20);
/// # Ok::<(), forgesync_engine::error::EngineError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClusterOptions {
    /// Minimum cosine similarity for an embedding edge.
    pub threshold: f64,
    /// Minimum cosine similarity for an issue-to-pull-request edge.
    pub cross_kind_threshold: f64,
    /// Maximum selected neighbors per discussion.
    pub fanout: usize,
    /// Maximum members in one generated component.
    pub max_cluster_size: usize,
    /// Minimum members emitted as a generated cluster.
    pub min_cluster_size: usize,
}

impl Default for ClusterOptions {
    /// Selects bounded neighbor/group sizes and distinct same-kind and cross-kind thresholds.
    /// Singleton groups are allowed; candidate and component policy enforce these limits later.
    fn default() -> Self {
        Self {
            threshold: DEFAULT_CLUSTER_THRESHOLD,
            cross_kind_threshold: DEFAULT_CROSS_KIND_THRESHOLD,
            fanout: 16,
            max_cluster_size: 40,
            min_cluster_size: 1,
        }
    }
}

impl ClusterOptions {
    /// Returns this policy after checking finite thresholds and bounded graph sizes.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::InvalidClusterOptions`] unless both thresholds are in `0..=1`,
    /// fanout is in `1..=256`, maximum size is in `1..=10_000`, and minimum size is positive
    /// and no larger than the maximum. Each threshold is validated independently.
    pub fn validate(self) -> Result<Self, EngineError> {
        if !self.threshold.is_finite()
            || !(0.0..=1.0).contains(&self.threshold)
            || !self.cross_kind_threshold.is_finite()
            || !(0.0..=1.0).contains(&self.cross_kind_threshold)
            || self.fanout == 0
            || self.fanout > 256
            || self.max_cluster_size == 0
            || self.max_cluster_size > 10_000
            || self.min_cluster_size == 0
            || self.min_cluster_size > self.max_cluster_size
        {
            return Err(EngineError::InvalidClusterOptions);
        }
        Ok(self)
    }
}

/// Explicit vector identity and graph policy for a local cluster generation.
#[derive(Clone, Debug)]
pub struct ClusterBuildRequest {
    /// Repository to cluster from current open discussions.
    pub repository: RepositorySelector,
    /// Canonical endpoint identity used when vectors were persisted, without credentials.
    pub endpoint: String,
    /// Model identity used when vectors were persisted.
    pub model: String,
    /// Versioned document recipe used when vectors were persisted.
    pub recipe: DocumentRecipe,
    /// Deterministic similarity and component limits.
    pub options: ClusterOptions,
}

/// Coverage and persistence result for one generated cluster run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ClusterBuildReport {
    /// Archive-local run and generated cluster counts.
    pub generation: ClusterGenerationResult,
    /// Number of current open discussions selected for this repository.
    pub eligible_threads: u64,
    /// Number of current open discussions with compatible stored vectors.
    pub vector_threads: u64,
    /// Candidate similarity and reference edges retained after neighbor pruning.
    pub candidate_edges: u64,
}

/// Repository filters and pagination for read-only cluster listing.
#[derive(Clone, Debug)]
pub struct ClusterListRequest {
    /// Repositories to include; empty selects every registered repository.
    pub repositories: Vec<RepositorySelector>,
    /// Include groups retired by a complete generation.
    pub include_retired: bool,
    /// Maximum result count, from 1 through 1000.
    pub limit: u32,
    /// Number of matching rows to skip.
    pub offset: u64,
}

mod build;
mod candidates;
mod components;
mod decisions;
mod evidence;
mod proposals;
mod references;
mod snapshot;

pub use build::{build_clusters, list_clusters};
pub use decisions::{
    dismiss_cluster, exclude_cluster_member, include_cluster_member, restore_cluster,
    set_canonical_cluster_member, show_cluster,
};

#[cfg(test)]
mod test_documents;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod threshold_tests;
