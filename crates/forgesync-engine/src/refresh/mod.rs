//! Compose sync with optional local analysis.
//!
//! Each selected stage reports separately, so a caller can see which work completed, failed, or
//! remained. Successful sync evidence survives a later analysis failure.

use forgesync_core::document::DocumentRecipe;
use forgesync_core::outcome::OperationOutcome;
use serde::Serialize;

use crate::clustering::{ClusterBuildReport, ClusterOptions};
use crate::embeddings::EmbeddingReport;
use crate::reference::RepositorySelector;
use crate::sync::{SyncReport, SyncThreadScope};

/// Names one stage in the composed refresh report.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
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
    /// A stage that produced no report.
    fn failed(failure: StageFailure) -> Self {
        Self {
            status: failure.status(),
            report: None,
            failure: Some(failure.failure),
        }
    }

    /// A stage with a (possibly partial) report and its primary failure.
    fn with_report(status: RefreshStageStatus, report: T, failure: Option<StageFailure>) -> Self {
        Self {
            status,
            report: Some(report),
            failure: failure.map(|failure| failure.failure),
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
///
/// All stages operate on `repositories`. Set `sync` to acquire provider evidence first, or leave
/// it absent for local analysis over existing archive content. Embeddings require a compatible
/// service identity, while cluster generation uses `cluster_options`; the report records each
/// selected stage separately.
#[derive(Clone, Debug)]
pub struct RefreshRequest {
    /// Repositories used by every selected stage.
    pub repositories: Vec<RepositorySelector>,
    /// Run GitHub acquisition, or omit it for an archive-only analysis refresh.
    pub sync: Option<RefreshSyncOptions>,
    /// Analysis stages (`Embeddings`, `Clusters`); they always run in that order.
    pub analysis: Vec<RefreshStageKind>,
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
///
/// A stage failure does not erase successful earlier work. Use `selected` to explain the request,
/// `outcome` for the aggregate status, and `remaining` to decide what may need another invocation.
/// Optional stage fields are absent when that stage was not requested.
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

mod clusters;
mod coordinator;
mod embeddings;
mod status;

pub use coordinator::refresh;
pub use embeddings::embed_repositories;
use status::StageFailure;
