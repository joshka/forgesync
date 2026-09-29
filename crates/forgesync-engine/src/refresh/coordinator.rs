//! Refresh coordinator behavior.

use super::clusters::build_repository_clusters;
use super::embeddings::{collect_embedding_repositories, embedding_status};
use super::status::{refresh_outcome, remaining_stages, stage_failure};
use super::{
    Archive, CancellationToken, DocumentRecipe, EmbeddingClient, EngineError, GitHubClient,
    GitHubHost, HashSet, OperationOutcome, RefreshAnalysisStage, RefreshEmbeddingReport,
    RefreshReport, RefreshRequest, RefreshStage, RefreshStageFailure, RefreshStageKind,
    RefreshStageStatus, RepositorySelector, SyncProgress, SyncReport, SyncRequest, mpsc,
    sync_repositories,
};

/// Runs selected refresh stages, retaining each stage's result if another stage fails.
///
/// The archive must already be open for writing. Cancellation is checked by the underlying
/// workflows; a partially completed stage remains visible in the returned report.
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

/// Rejects refresh selections that cannot run before any stage starts.
pub fn validate_request(request: &RefreshRequest) -> Result<(), EngineError> {
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

/// Keeps the first occurrence of each requested repository in stage order.
pub fn unique_repositories(repositories: &[RepositorySelector]) -> Vec<RepositorySelector> {
    let mut seen = HashSet::with_capacity(repositories.len());
    repositories
        .iter()
        .filter(|repository| seen.insert(repository.as_url()))
        .cloned()
        .collect()
}

/// Preserves a sync report and its structured outcome as one refresh stage.
pub fn stage_from_sync_report(report: SyncReport) -> RefreshStage<SyncReport> {
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
