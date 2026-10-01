//! Run selected refresh stages in dependency order.
//!
//! Acquisition always precedes embeddings, which precede clustering. Each selected stage is
//! attempted and reported independently; a failure neither erases earlier durable progress nor
//! suppresses later analysis, which judges available evidence through its own workflow.

use std::collections::{HashMap, HashSet};

use forgesync_core::identity::GitHubHost;
use forgesync_core::outcome::OperationOutcome;
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::embedding_client::EmbeddingClient;
use crate::embeddings::EmbeddingPolicy;
use crate::error::EngineError;
use crate::reference::RepositorySelector;
use crate::refresh::clusters::build_repository_clusters;
use crate::refresh::embeddings::embed_repositories;
use crate::refresh::status::StageFailure;
use crate::refresh::{
    RefreshReport, RefreshRequest, RefreshStage, RefreshStageFailure, RefreshStageKind,
};
use crate::sync::{SyncProgress, SyncReport, SyncRequest, sync_repositories};

/// Runs selected refresh stages, retaining each stage's result if another stage fails.
///
/// The archive must already be open for writing. A returned report can contain failed or
/// interrupted stages even though this returns `Ok`; only invalid scope is an error.
pub async fn refresh(
    archive: &Archive,
    github_clients: &HashMap<GitHubHost, GitHubClient>,
    embedding_client: Option<&EmbeddingClient>,
    request: &RefreshRequest,
    cancellation: &CancellationToken,
    progress: Option<mpsc::Sender<SyncProgress>>,
) -> Result<RefreshReport, EngineError> {
    let mut seen = HashSet::new();
    let analysis_valid = request
        .analysis
        .iter()
        .all(|stage| *stage != RefreshStageKind::Sync && seen.insert(*stage));
    if request.repositories.is_empty()
        || (request.sync.is_none() && request.analysis.is_empty())
        || !analysis_valid
    {
        return Err(EngineError::InvalidRefreshScope);
    }
    let repositories = unique_repositories(&request.repositories);
    let selects = |stage| request.analysis.contains(&stage);

    let sync = match request.sync {
        Some(options) => {
            let sync_request = SyncRequest {
                repositories: repositories.clone(),
                all: false,
                scope: options.scope,
                include_comments: options.include_comments,
                include_reviews: options.include_reviews,
                include_review_threads: options.include_review_threads,
                parent_run: None,
            };
            let result = sync_repositories(
                archive,
                github_clients,
                &sync_request,
                cancellation,
                progress,
            )
            .await;
            Some(match result {
                Ok(report) => stage_from_sync_report(report),
                Err(error) => RefreshStage::failed(StageFailure::from_error(&error)),
            })
        }
        None => None,
    };
    let embeddings = if selects(RefreshStageKind::Embeddings) {
        Some(match embedding_client {
            Some(client) => {
                let policy = EmbeddingPolicy::from_force(request.force_embeddings);
                embed_repositories(
                    archive,
                    &repositories,
                    client,
                    request.recipe,
                    policy,
                    cancellation,
                )
                .await
            }
            None => RefreshStage::failed(StageFailure::failed(RefreshStageFailure {
                code: "embedding_service_unavailable",
                message: "embedding analysis requires a valid configured embedding service"
                    .to_owned(),
            })),
        })
    } else {
        None
    };
    let clusters = if selects(RefreshStageKind::Clusters) {
        let stage = build_repository_clusters(
            archive,
            &repositories,
            request.embedding_identity.as_ref(),
            request.recipe,
            request.cluster_options,
            cancellation,
        );
        Some(stage.await)
    } else {
        None
    };

    let selected = [
        (RefreshStageKind::Sync, sync.is_some()),
        (RefreshStageKind::Embeddings, embeddings.is_some()),
        (RefreshStageKind::Clusters, clusters.is_some()),
    ];
    let mut report = RefreshReport {
        selected: selected
            .into_iter()
            .filter_map(|(kind, selected)| selected.then_some(kind))
            .collect(),
        sync,
        embeddings,
        clusters,
        remaining: Vec::new(),
        outcome: OperationOutcome::Complete,
    };
    report.finish();
    Ok(report)
}

/// Keeps the first occurrence of each requested repository.
fn unique_repositories(repositories: &[RepositorySelector]) -> Vec<RepositorySelector> {
    let mut seen = HashSet::with_capacity(repositories.len());
    repositories
        .iter()
        .filter(|repository| seen.insert(repository.as_url()))
        .cloned()
        .collect()
}

/// Preserves a sync report and its structured outcome as one refresh stage.
fn stage_from_sync_report(report: SyncReport) -> RefreshStage<SyncReport> {
    let failure = match &report.outcome {
        OperationOutcome::Failed { failure } => Some(StageFailure::failed(RefreshStageFailure {
            code: "sync_failed",
            message: failure.message.clone(),
        })),
        _ => None,
    };
    RefreshStage::with_report((&report.outcome).into(), report, failure)
}
