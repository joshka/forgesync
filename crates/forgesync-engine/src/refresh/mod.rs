//! Coordinated sync and analysis stages.

use std::collections::HashSet;

use forgesync_core::document::DocumentRecipe;
use forgesync_core::identity::GitHubHost;
use forgesync_core::outcome::OperationOutcome;
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::clustering::{ClusterBuildReport, ClusterBuildRequest, ClusterOptions, build_clusters};
use crate::documents::materialize_thread_document;
use crate::embedding_client::EmbeddingClient;
use crate::embeddings::{EmbeddingReport, embed_documents};
use crate::error::EngineError;
use crate::inspect::{
    ThreadFilters, ThreadListRequest, ThreadSort, ThreadStateFilter, list_threads,
};
use crate::reference::{RepositorySelector, ThreadSelector};
use crate::sync::{SyncProgress, SyncReport, SyncRequest, SyncThreadScope, sync_repositories};

/// Selects the optional model-backed stages included in a refresh.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshAnalysisStage {
    /// Materialize current discussion documents and request missing embeddings.
    Embeddings,
    /// Build local clusters from compatible vectors already in the archive.
    Clusters,
}

/// Names one stage in the composed refresh report.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshStageKind {
    /// Acquire selected GitHub evidence.
    Sync,
    /// Materialize documents and acquire missing vectors.
    Embeddings,
    /// Generate clusters from stored vectors.
    Clusters,
}

/// Result state for one selected refresh stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshStageStatus {
    /// Every item selected by this stage completed.
    Complete,
    /// Some work completed, but failures or incomplete coverage remain.
    Partial,
    /// The stage could not produce a usable result.
    Failed,
    /// Cancellation stopped the stage with work remaining.
    Interrupted,
    /// Policy deferred the selected work.
    Deferred,
}

/// Safe failure detail retained alongside a refresh stage result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RefreshStageFailure {
    /// Stable machine-readable error classification.
    pub code: &'static str,
    /// Safe short description suitable for command output.
    pub message: String,
}

/// State and optional report for one selected stage.
#[derive(Clone, Debug, Serialize)]
pub struct RefreshStage<T> {
    /// Terminal state of this stage.
    pub status: RefreshStageStatus,
    /// Partial or complete stage output, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<T>,
    /// First stage-level failure that needs attention.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<RefreshStageFailure>,
}

impl<T> RefreshStage<T> {
    fn failed(failure: RefreshStageFailure) -> Self {
        Self {
            status: status_for_failure(&failure),
            report: None,
            failure: Some(failure),
        }
    }

    fn with_report(
        status: RefreshStageStatus,
        report: T,
        failure: Option<RefreshStageFailure>,
    ) -> Self {
        Self {
            status,
            report: Some(report),
            failure,
        }
    }
}

/// Minimal embedding service identity used to select already stored vectors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingServiceIdentity {
    /// Canonical base endpoint identity, without credentials.
    pub endpoint: String,
    /// Model identity stored with compatible vectors.
    pub model: String,
}

/// Sync-only controls used when sync is one of the selected refresh stages.
#[derive(Clone, Copy, Debug)]
pub struct RefreshSyncOptions {
    /// Thread state and closed-sweep policy.
    pub scope: SyncThreadScope,
    /// Acquire issue and pull-request discussion comments.
    pub include_comments: bool,
    /// Acquire pull-request reviews.
    pub include_reviews: bool,
    /// Acquire pull-request review threads.
    pub include_review_threads: bool,
}

/// Inputs for composing selected local and remote refresh stages.
#[derive(Clone, Debug)]
pub struct RefreshRequest {
    /// Repositories used by every selected stage.
    pub repositories: Vec<RepositorySelector>,
    /// Run GitHub acquisition, or omit it for an archive-only analysis refresh.
    pub sync: Option<RefreshSyncOptions>,
    /// Optional model-backed analysis stages in execution order.
    pub analysis: Vec<RefreshAnalysisStage>,
    /// Document recipe used by document materialization and vector matching.
    pub recipe: DocumentRecipe,
    /// Service identity used to select compatible stored vectors.
    pub embedding_identity: Option<EmbeddingServiceIdentity>,
    /// Force provider requests for every selected document chunk.
    pub force_embeddings: bool,
    /// Deterministic policy for generated cluster groups.
    pub cluster_options: ClusterOptions,
}

/// A document that could not be materialized during the embedding stage.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RefreshDocumentFailure {
    /// Repository containing the discussion.
    pub repository: String,
    /// Positive repository-local discussion number.
    pub number: u64,
    /// Stable machine-readable error classification.
    pub code: &'static str,
    /// Safe short description suitable for command output.
    pub message: String,
}

/// Document materialization results combined with embedding outcomes.
#[derive(Clone, Debug, Default, Serialize)]
pub struct RefreshEmbeddingReport {
    /// Aggregate embedding work across selected repositories.
    pub embeddings: EmbeddingReport,
    /// Documents successfully built and stored before embedding.
    pub documents_materialized: usize,
    /// Per-discussion document failures; other discussions continue independently.
    pub document_failures: Vec<RefreshDocumentFailure>,
}

/// Cluster output or failure for one selected repository.
#[derive(Clone, Debug, Serialize)]
pub struct RefreshClusterRepository {
    /// Stable repository URL for this result.
    pub repository: String,
    /// Generated cluster report when this repository completed a build.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<ClusterBuildReport>,
    /// Failure detail when this repository could not produce a cluster report.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<RefreshStageFailure>,
}

/// Complete selection, per-stage results, and remaining work for one refresh.
#[derive(Clone, Debug, Serialize)]
pub struct RefreshReport {
    /// Stages selected by this request, in execution order.
    pub selected: Vec<RefreshStageKind>,
    /// GitHub acquisition result when sync was selected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync: Option<RefreshStage<SyncReport>>,
    /// Document and vector result when embeddings were selected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeddings: Option<RefreshStage<RefreshEmbeddingReport>>,
    /// Per-repository cluster result when clustering was selected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clusters: Option<RefreshStage<Vec<RefreshClusterRepository>>>,
    /// Selected stages that may need to be retried or completed.
    pub remaining: Vec<RefreshStageKind>,
    /// Aggregate outcome across all selected stages.
    pub outcome: OperationOutcome,
}

/// Runs sync and explicitly selected analysis stages while retaining every stage result.
///
/// Failures in one stage do not discard completed work or prevent later independent stages from
/// running. Invalid top-level scope is rejected before any work starts.
mod clusters;
mod coordinator;
mod embeddings;
mod status;

use clusters::*;
pub use coordinator::{embed_repositories, refresh};
use embeddings::*;
use status::*;
