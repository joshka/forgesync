use std::collections::HashSet;

use forgesync_core::document::DocumentRecipe;
use forgesync_core::identity::GitHubHost;
use forgesync_core::outcome::OperationOutcome;
use forgesync_github::transport::GitHubClient;
use forgesync_store::Archive;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::clustering::{ClusterBuildReport, ClusterBuildRequest, ClusterOptions, build_clusters};
use crate::documents::materialize_thread_document;
use crate::embeddings::{EmbeddingReport, embed_documents};
use crate::inspect::{
    ThreadFilters, ThreadListRequest, ThreadSort, ThreadStateFilter, list_threads,
};
use crate::sync::{SyncProgress, SyncReport, SyncRequest, SyncThreadScope, sync_repositories};
use crate::{EmbeddingClient, EngineError, RepositorySelector, ThreadSelector};

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
pub async fn refresh(
    archive: &Archive,
    github_clients: &std::collections::HashMap<GitHubHost, GitHubClient>,
    embedding_client: Option<&EmbeddingClient>,
    request: &RefreshRequest,
    cancellation: &CancellationToken,
    progress: Option<mpsc::Sender<SyncProgress>>,
) -> Result<RefreshReport, EngineError> {
    validate_request(request)?;

    let repositories = unique_repositories(&request.repositories);
    let mut selected = Vec::with_capacity(1 + request.analysis.len());
    if request.sync.is_some() {
        selected.push(RefreshStageKind::Sync);
    }
    if request.analysis.contains(&RefreshAnalysisStage::Embeddings) {
        selected.push(RefreshStageKind::Embeddings);
    }
    if request.analysis.contains(&RefreshAnalysisStage::Clusters) {
        selected.push(RefreshStageKind::Clusters);
    }

    let sync = if let Some(options) = request.sync {
        let sync_request = SyncRequest {
            repositories: repositories.clone(),
            all: false,
            scope: options.scope,
            include_comments: options.include_comments,
            include_reviews: options.include_reviews,
            include_review_threads: options.include_review_threads,
            parent_run: None,
        };
        let stage = match sync_repositories(
            archive,
            github_clients,
            &sync_request,
            cancellation,
            progress,
        )
        .await
        {
            Ok(report) => stage_from_sync_report(report),
            Err(error) => RefreshStage::failed(stage_failure(&error)),
        };
        Some(stage)
    } else {
        None
    };

    let embeddings = if request.analysis.contains(&RefreshAnalysisStage::Embeddings) {
        Some(match embedding_client {
            Some(client) => {
                embed_repositories(
                    archive,
                    &repositories,
                    client,
                    request.recipe,
                    request.force_embeddings,
                    cancellation,
                )
                .await
            }
            None => RefreshStage::failed(RefreshStageFailure {
                code: "embedding_service_unavailable",
                message: "embedding analysis requires a valid configured embedding service"
                    .to_owned(),
            }),
        })
    } else {
        None
    };

    let clusters = if request.analysis.contains(&RefreshAnalysisStage::Clusters) {
        Some(
            build_repository_clusters(
                archive,
                &repositories,
                request.embedding_identity.as_ref(),
                request.recipe,
                request.cluster_options,
                cancellation,
            )
            .await,
        )
    } else {
        None
    };

    let mut report = RefreshReport {
        selected,
        sync,
        embeddings,
        clusters,
        remaining: Vec::new(),
        outcome: OperationOutcome::Complete,
    };
    report.remaining = remaining_stages(&report);
    report.outcome = refresh_outcome(&report);
    Ok(report)
}

/// Materializes current repository documents and embeds their missing compatible chunks.
///
/// Repository and document failures are retained in the stage report so callers can present
/// successful work and retry guidance without losing already stored batches.
pub async fn embed_repositories(
    archive: &Archive,
    repositories: &[RepositorySelector],
    client: &EmbeddingClient,
    recipe: DocumentRecipe,
    force: bool,
    cancellation: &CancellationToken,
) -> RefreshStage<RefreshEmbeddingReport> {
    let (report, failure) =
        collect_embedding_repositories(archive, repositories, client, recipe, force, cancellation)
            .await;
    let status = embedding_status(&report, failure.as_ref());
    RefreshStage::with_report(status, report, failure)
}

fn validate_request(request: &RefreshRequest) -> Result<(), EngineError> {
    let mut seen_stages = HashSet::with_capacity(request.analysis.len());
    let unique_stages = request
        .analysis
        .iter()
        .all(|stage| seen_stages.insert(*stage));
    if request.repositories.is_empty()
        || (request.sync.is_none() && request.analysis.is_empty())
        || !unique_stages
    {
        return Err(EngineError::InvalidRefreshScope);
    }
    Ok(())
}

fn unique_repositories(repositories: &[RepositorySelector]) -> Vec<RepositorySelector> {
    let mut seen = HashSet::with_capacity(repositories.len());
    repositories
        .iter()
        .filter(|repository| seen.insert(repository.as_url()))
        .cloned()
        .collect()
}

fn stage_from_sync_report(report: SyncReport) -> RefreshStage<SyncReport> {
    let status = match report.outcome {
        OperationOutcome::Complete => RefreshStageStatus::Complete,
        OperationOutcome::Partial { .. } => RefreshStageStatus::Partial,
        OperationOutcome::Deferred { .. } => RefreshStageStatus::Deferred,
        OperationOutcome::Failed { .. } => RefreshStageStatus::Failed,
        OperationOutcome::Interrupted { .. } => RefreshStageStatus::Interrupted,
    };
    let failure = match &report.outcome {
        OperationOutcome::Failed { failure } => Some(RefreshStageFailure {
            code: "sync_failed",
            message: failure.message.clone(),
        }),
        _ => None,
    };
    RefreshStage::with_report(status, report, failure)
}

async fn collect_embedding_repositories(
    archive: &Archive,
    repositories: &[RepositorySelector],
    client: &EmbeddingClient,
    recipe: DocumentRecipe,
    force: bool,
    cancellation: &CancellationToken,
) -> (RefreshEmbeddingReport, Option<RefreshStageFailure>) {
    let mut result = RefreshEmbeddingReport::default();
    let mut first_failure = None;

    for repository in repositories {
        let mut offset = 0_u64;
        loop {
            let page = match list_threads(
                archive,
                &ThreadListRequest {
                    filters: ThreadFilters {
                        repositories: vec![repository.clone()],
                        kind: None,
                        state: ThreadStateFilter::All,
                        sort: Some(ThreadSort::Updated),
                        limit: 1000,
                        offset,
                    },
                },
            )
            .await
            {
                Ok(page) => page,
                Err(error) => {
                    keep_first_failure(&mut first_failure, stage_failure(&error));
                    break;
                }
            };

            let next_offset = page.next_offset;
            let mut documents = Vec::with_capacity(page.items.len());
            for thread in page.items {
                let selector = ThreadSelector::new(
                    RepositorySelector::from_repository(&thread.repository),
                    thread.discussion.id.number(),
                );
                match materialize_thread_document(archive, &selector, recipe).await {
                    Ok(built) => {
                        documents.push(built.document);
                        result.documents_materialized =
                            result.documents_materialized.saturating_add(1);
                    }
                    Err(error) => {
                        result.document_failures.push(RefreshDocumentFailure {
                            repository: repository.as_url(),
                            number: thread.discussion.id.number().get(),
                            code: error.code(),
                            message: error.to_string(),
                        });
                    }
                }
                if cancellation.is_cancelled() {
                    break;
                }
            }

            if !documents.is_empty() {
                match embed_documents(archive, client, &documents, force, cancellation).await {
                    Ok(report) => add_embedding_report(&mut result.embeddings, report),
                    Err(error) => {
                        keep_first_failure(&mut first_failure, stage_failure(&error));
                    }
                }
            }

            if cancellation.is_cancelled() {
                result.embeddings.cancelled = true;
                break;
            }
            let Some(next_offset) = next_offset else {
                break;
            };
            offset = next_offset;
        }
        if cancellation.is_cancelled() {
            break;
        }
    }

    (result, first_failure)
}

async fn build_repository_clusters(
    archive: &Archive,
    repositories: &[RepositorySelector],
    identity: Option<&EmbeddingServiceIdentity>,
    recipe: DocumentRecipe,
    options: ClusterOptions,
    cancellation: &CancellationToken,
) -> RefreshStage<Vec<RefreshClusterRepository>> {
    let mut results = Vec::with_capacity(repositories.len());
    let mut first_failure = None;
    let mut has_partial_coverage = false;

    for repository in repositories {
        if cancellation.is_cancelled() {
            keep_first_failure(
                &mut first_failure,
                RefreshStageFailure {
                    code: "operation_cancelled",
                    message: "clustering was cancelled with repositories remaining".to_owned(),
                },
            );
            break;
        }

        let Some(identity) = identity else {
            let failure = RefreshStageFailure {
                code: "embedding_service_identity_missing",
                message: "clustering requires an endpoint and model matching stored vectors"
                    .to_owned(),
            };
            keep_first_failure(&mut first_failure, failure.clone());
            results.push(RefreshClusterRepository {
                repository: repository.as_url(),
                report: None,
                failure: Some(failure),
            });
            continue;
        };

        let request = ClusterBuildRequest {
            repository: repository.clone(),
            endpoint: identity.endpoint.clone(),
            model: identity.model.clone(),
            recipe,
            options,
        };
        match build_clusters(archive, &request, cancellation).await {
            Ok(report) => {
                has_partial_coverage |= !report.generation.complete_coverage;
                results.push(RefreshClusterRepository {
                    repository: repository.as_url(),
                    report: Some(report),
                    failure: None,
                });
            }
            Err(error) => {
                let failure = stage_failure(&error);
                keep_first_failure(&mut first_failure, failure.clone());
                results.push(RefreshClusterRepository {
                    repository: repository.as_url(),
                    report: None,
                    failure: Some(failure),
                });
                if cancellation.is_cancelled() {
                    break;
                }
            }
        }
    }

    let status = if first_failure
        .as_ref()
        .is_some_and(|failure| failure.code == "operation_cancelled")
    {
        RefreshStageStatus::Interrupted
    } else if first_failure.is_some() {
        if results.iter().any(|result| result.report.is_some()) {
            RefreshStageStatus::Partial
        } else {
            RefreshStageStatus::Failed
        }
    } else if has_partial_coverage {
        RefreshStageStatus::Partial
    } else {
        RefreshStageStatus::Complete
    };
    RefreshStage::with_report(status, results, first_failure)
}

fn embedding_status(
    report: &RefreshEmbeddingReport,
    failure: Option<&RefreshStageFailure>,
) -> RefreshStageStatus {
    if report.embeddings.cancelled
        || failure.is_some_and(|failure| failure.code == "operation_cancelled")
    {
        RefreshStageStatus::Interrupted
    } else if failure.is_some()
        || !report.document_failures.is_empty()
        || !report.embeddings.failed_batches.is_empty()
    {
        if report.documents_materialized > 0
            || report.embeddings.chunks_embedded > 0
            || report.embeddings.chunks_skipped > 0
        {
            RefreshStageStatus::Partial
        } else {
            RefreshStageStatus::Failed
        }
    } else {
        RefreshStageStatus::Complete
    }
}

fn add_embedding_report(total: &mut EmbeddingReport, page: EmbeddingReport) {
    total.documents = total.documents.saturating_add(page.documents);
    total.chunks_selected = total.chunks_selected.saturating_add(page.chunks_selected);
    total.chunks_embedded = total.chunks_embedded.saturating_add(page.chunks_embedded);
    total.chunks_skipped = total.chunks_skipped.saturating_add(page.chunks_skipped);
    total.failed_batches.extend(page.failed_batches);
    total.cancelled |= page.cancelled;
}

fn stage_failure(error: &EngineError) -> RefreshStageFailure {
    RefreshStageFailure {
        code: error.code(),
        message: error.to_string(),
    }
}

fn keep_first_failure(first: &mut Option<RefreshStageFailure>, candidate: RefreshStageFailure) {
    if first.is_none() {
        *first = Some(candidate);
    }
}

fn status_for_failure(failure: &RefreshStageFailure) -> RefreshStageStatus {
    if failure.code == "operation_cancelled" {
        RefreshStageStatus::Interrupted
    } else {
        RefreshStageStatus::Failed
    }
}

fn remaining_stages(report: &RefreshReport) -> Vec<RefreshStageKind> {
    let mut remaining = Vec::new();
    if report
        .sync
        .as_ref()
        .is_some_and(|stage| stage.status != RefreshStageStatus::Complete)
    {
        remaining.push(RefreshStageKind::Sync);
    }
    if report
        .embeddings
        .as_ref()
        .is_some_and(|stage| stage.status != RefreshStageStatus::Complete)
    {
        remaining.push(RefreshStageKind::Embeddings);
    }
    if report
        .clusters
        .as_ref()
        .is_some_and(|stage| stage.status != RefreshStageStatus::Complete)
    {
        remaining.push(RefreshStageKind::Clusters);
    }
    remaining
}

fn refresh_outcome(report: &RefreshReport) -> OperationOutcome {
    if report.remaining.is_empty() {
        return OperationOutcome::Complete;
    }
    if report
        .remaining
        .iter()
        .any(|stage| stage_status(report, *stage) == Some(RefreshStageStatus::Interrupted))
    {
        return OperationOutcome::Interrupted {
            pending_items: u64::try_from(report.remaining.len()).unwrap_or(u64::MAX),
        };
    }
    let failed_items = u64::try_from(report.remaining.len()).unwrap_or(u64::MAX);
    let deferred_items = u64::try_from(
        report
            .remaining
            .iter()
            .filter(|stage| stage_status(report, **stage) == Some(RefreshStageStatus::Deferred))
            .count(),
    )
    .unwrap_or(u64::MAX);
    OperationOutcome::Partial {
        failed_items,
        deferred_items,
    }
}

fn stage_status(report: &RefreshReport, stage: RefreshStageKind) -> Option<RefreshStageStatus> {
    match stage {
        RefreshStageKind::Sync => report.sync.as_ref().map(|stage| stage.status),
        RefreshStageKind::Embeddings => report.embeddings.as_ref().map(|stage| stage.status),
        RefreshStageKind::Clusters => report.clusters.as_ref().map(|stage| stage.status),
    }
}
