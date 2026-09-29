//! # Stored duplicate clusters and maintainer decisions
//!
//! A cluster generation is a derived grouping of archived threads. `ClusterGenerationInput`,
//! `ClusterInput`, and `ClusterMemberInput` describe what the engine proposes to store;
//! `ClusterGenerationResult` reports the committed generation. Summary and detail types are read
//! projections for CLI and TUI callers.
//!
//! Lifecycle, member state, and member role are separate because a proposed relationship and a
//! maintainer decision have different meanings. `generation` writes proposed membership, `queries`
//! reads it, and `decisions` records local triage actions. The engine owns candidate selection;
//! this module owns persistence and the durable effect of decisions.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;

use forgesync_core::content::Repository;
use forgesync_core::document::DocumentRecipe;
use forgesync_core::identity::{RepositoryId, ThreadId, ThreadNumber, ThreadReference};
use forgesync_core::timestamp::UtcTimestamp;
use serde::Serialize;
use sha2::Sha256;
use sqlx::{QueryBuilder, Sqlite, SqliteConnection};

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::reads::{ThreadSummary, coverage_for_kind, load_thread_coverage};

/// One generated member and its default score to the graph representative.
#[derive(Clone, Debug)]
pub struct ClusterMemberInput {
    /// Stable source discussion identity.
    pub thread: ThreadId,
    /// Cosine or deterministic reference score to the generated representative.
    pub score_to_representative: Option<f64>,
}

/// One connected component produced by the deterministic clustering engine.
#[derive(Clone, Debug)]
pub struct ClusterInput {
    /// Graph-selected representative before local canonical decisions are applied.
    pub representative: ThreadId,
    /// Display title derived from the representative's current source title.
    pub title: String,
    /// Current generated members in stable identity order.
    pub members: Vec<ClusterMemberInput>,
}

/// Complete or partial cluster generation input for one repository and vector service.
#[derive(Clone, Debug)]
pub struct ClusterGenerationInput {
    /// Repository whose current open discussions were clustered.
    pub repository: RepositoryId,
    /// Compatible embedding endpoint identity, without credentials.
    pub endpoint: String,
    /// Compatible embedding model identity.
    pub model: String,
    /// Document recipe used for the vectors.
    pub recipe: DocumentRecipe,
    /// True only when every eligible current discussion has a compatible vector.
    pub complete_coverage: bool,
    /// Number of eligible discussions in the requested state scope.
    pub eligible_threads: u64,
    /// Number of eligible discussions with compatible current vectors.
    pub vector_threads: u64,
    /// Number of similarity and reference edges retained after fanout pruning.
    pub candidate_edges: u64,
    /// Generated clusters, including single-member orphan clusters when selected.
    pub clusters: Vec<ClusterInput>,
}

/// Durable counts and run identity produced by saving one cluster generation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ClusterGenerationResult {
    /// Archive-local cluster run ID.
    pub run_id: u64,
    /// Number of generated clusters in this run.
    pub cluster_count: u64,
    /// Number of generated memberships in this run.
    pub member_count: u64,
    /// Number of previous generated clusters retired by a complete run.
    pub retired_count: u64,
    /// Whether this run had complete vector coverage.
    pub complete_coverage: bool,
}

/// Whether a generated cluster is current or retained as historical context.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClusterLifecycle {
    /// The cluster belongs to the latest complete or partial generation.
    Active,
    /// A complete generation no longer contains this cluster.
    Retired,
}

/// Current state of one generated cluster membership.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClusterMemberState {
    /// Included in the visible generated membership.
    Active,
    /// Excluded by a local maintainer decision.
    Excluded,
    /// No longer in a complete regenerated component.
    Removed,
}

/// Effective display role of one current cluster member.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClusterMemberRole {
    /// Selected by an explicit local canonical decision.
    Canonical,
    /// Selected by the generated graph representative rule.
    Representative,
    /// A related cluster member.
    Related,
}

/// Summary of one persisted generated cluster.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ClusterSummary {
    /// Stable archive-local public cluster ID.
    pub id: u64,
    /// Current repository identity and display metadata.
    pub repository: Repository,
    /// Display title from the graph-selected representative.
    pub title: String,
    /// Active or retired generated state.
    pub lifecycle: ClusterLifecycle,
    /// Whether a maintainer locally dismissed this cluster.
    pub dismissed: bool,
    /// Reason supplied for a local dismissal, when dismissed.
    pub dismissal_reason: Option<String>,
    /// Effective canonical or generated representative.
    pub representative: Option<ThreadReference>,
    /// Number of active, non-excluded generated members.
    pub active_member_count: u64,
    /// Number of locally excluded members still present in this cluster.
    pub excluded_member_count: u64,
    /// Latest cluster generation that updated this identity.
    pub last_run_id: Option<u64>,
    /// Latest generation or local decision time.
    pub updated_at: UtcTimestamp,
}

/// A generated member with current local decision state.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ClusterMember {
    /// Current discussion and its evidence coverage.
    pub summary: ThreadSummary,
    /// Effective display role after local canonical selection.
    pub role: ClusterMemberRole,
    /// Current membership state.
    pub state: ClusterMemberState,
    /// Generated score relative to the graph-selected representative.
    pub score_to_representative: Option<f64>,
}

/// A persisted cluster and its current or locally excluded members.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ClusterDetail {
    /// Public cluster summary.
    pub cluster: ClusterSummary,
    /// Current active or excluded members in stable number order.
    pub members: Vec<ClusterMember>,
}

/// Repository scope and pagination for generated cluster listings.
pub struct ClusterListQuery<'a> {
    /// Resolved repository identities; an empty slice includes every repository.
    pub repositories: &'a [RepositoryId],
    /// Include clusters retired by a complete generation.
    pub include_retired: bool,
    /// Maximum number of rows, from 1 through 1000.
    pub limit: NonZeroU32,
    /// Number of matching clusters to skip.
    pub offset: u64,
}

/// One page of cluster summaries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ClusterPage {
    /// Cluster summaries in stable size and ID order.
    pub items: Vec<ClusterSummary>,
    /// Offset to pass to the next request, when another page is available.
    pub next_offset: Option<u64>,
}

struct PreparedCluster {
    representative_id: i64,
    title: String,
    members: Vec<(i64, Option<f64>)>,
}

struct ExistingCluster {
    id: i64,
    members: HashSet<i64>,
}

mod decisions;
mod generation;
mod queries;

use decisions::{checked_cluster_id, insert_cluster_event};
use generation::thread_row_id;
