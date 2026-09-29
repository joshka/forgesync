//! Duplicate cluster generation and maintainer decisions.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::{Arc, LazyLock, OnceLock};
use std::time::Duration;

use forgesync_core::document::DocumentRecipe;
use forgesync_core::identity::{RepositoryId, ThreadId};
use forgesync_store::archive::Archive;
use forgesync_store::clusters::{
    ClusterDetail, ClusterGenerationInput, ClusterGenerationResult, ClusterInput,
    ClusterListQuery as StoreClusterListQuery, ClusterMemberInput, ClusterPage,
};
use forgesync_store::embeddings::{EmbeddingDocumentQuery, EmbeddingSearchDocument};
use forgesync_store::leases::ArchiveLeaseToken;
use forgesync_store::reads::{ThreadQuery, ThreadSort, ThreadStateFilter, ThreadSummary};
use regex::Regex;
use serde::Serialize;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{Instant, interval_at};
use tokio_util::sync::CancellationToken;

use crate::documents::now_utc;
use crate::error::EngineError;
use crate::exact_search::{cosine_similarity, stable_thread_id_cmp};
use crate::inspect::{checked_page, resolve_repositories};
use crate::reference::{RepositorySelector, ThreadSelector};

const CLUSTER_PAGE_SIZE: u32 = 500;
const CLUSTER_LEASE_DURATION: Duration = Duration::from_secs(180);
const CLUSTER_WORKER_LIMIT: usize = 1;

static CLUSTER_WORKER_SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();

const DEFAULT_CLUSTER_THRESHOLD: f64 = 0.80;
const DEFAULT_CROSS_KIND_THRESHOLD: f64 = 0.93;
const HIGH_CONFIDENCE_SCORE: f64 = 0.90;
const MIN_TITLE_OVERLAP: f64 = 0.18;
const REFERENCE_SCORE: f64 = 0.94;
const EARLY_BODY_REFERENCE_BYTES: usize = 240;

static TITLE_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z0-9]{4,}").expect("valid title token pattern"));
static THREAD_REFERENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:\b([\w.-]+/[\w.-]+)#(\d+)|(?:\b([\w.-]+/[\w.-]+)/)?(?:issues|pull)/(\d+)|#(\d{2,}))")
        .expect("valid issue reference pattern")
});

/// Tuning options for deterministic related-discussion clustering.
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
    pub(crate) fn validate(self) -> Result<Self, EngineError> {
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

#[derive(Clone, Debug)]
pub(crate) struct ClusterMemberCandidate {
    pub summary: ThreadSummary,
    pub score_to_representative: Option<f64>,
}

#[derive(Clone, Debug)]
pub(crate) struct ClusterCandidate {
    pub representative: ThreadId,
    pub title: String,
    pub members: Vec<ClusterMemberCandidate>,
}

#[derive(Clone, Copy, Debug)]
struct Neighbor {
    node_index: usize,
    score: f64,
}

impl PartialEq for Neighbor {
    fn eq(&self, other: &Self) -> bool {
        self.node_index == other.node_index && self.score.total_cmp(&other.score) == Ordering::Equal
    }
}

impl Eq for Neighbor {}

impl PartialOrd for Neighbor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Neighbor {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .score
            .total_cmp(&self.score)
            .then_with(|| self.node_index.cmp(&other.node_index))
    }
}

#[derive(Clone, Copy, Debug)]
struct CandidateEdge {
    left: usize,
    right: usize,
    score: f64,
}

mod build;
mod candidates;
mod decisions;
mod lease;

pub use build::{build_clusters, list_clusters};
use candidates::build_cluster_candidates;
pub use decisions::{
    dismiss_cluster, exclude_cluster_member, include_cluster_member, restore_cluster,
    set_canonical_cluster_member, show_cluster,
};
use lease::{finish_cluster_decision_lease, finish_cluster_lease, finish_cluster_lease_result};

#[cfg(test)]
mod tests;
