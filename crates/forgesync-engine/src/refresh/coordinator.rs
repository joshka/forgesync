//! # Run selected refresh stages in dependency order
//!
//! [`refresh`] validates stage selection and builds a private execution owner around the supplied
//! archive, provider clients, optional embedding client, request, and cancellation token.
//! Repository selectors are deduplicated by their formatted URL while retaining first-occurrence
//! order. Selection validation rejects empty scope, no stages, and repeated analysis stages; it
//! does not prove that repositories exist or that all services are available.
//!
//! Execution always orders selected acquisition, embeddings, then clustering. Each selected stage
//! is attempted and reports independently; failure does not erase earlier durable progress or
//! automatically suppress later analysis. Later stages must judge available evidence through their
//! own workflow boundaries. Cancellation is passed to those workflows rather than enforced by a
//! single coordinator transaction.
//!
//! The embedding adapter retains document and batch outcomes; the cluster adapter retains results
//! per repository. Status helpers derive remaining work and the combined outcome only after these
//! stage reports are assembled. Progress delivery here is the optional sync progress channel, not
//! a unified stream of every derived-analysis event.
//!
//! Callers open and close the archive and prepare clients. The coordinator installs no process
//! diagnostics, discovers no credentials, and holds no transaction across provider I/O. A returned
//! report can contain failed or interrupted stages even though request execution returned `Ok`.

use std::collections::HashSet;

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
use crate::refresh::status::{refresh_outcome, remaining_stages, stage_failure};
use crate::refresh::{
    RefreshAnalysisStage, RefreshEmbeddingReport, RefreshReport, RefreshRequest, RefreshStage,
    RefreshStageFailure, RefreshStageKind, RefreshStageStatus,
};
use crate::sync::{SyncProgress, SyncReport, SyncRequest, sync_repositories};

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
    let execution = RefreshExecution {
        archive,
        github_clients,
        embedding_client,
        request,
        cancellation,
        progress,
        repositories: unique_repositories(&request.repositories),
    };
    Ok(execution.run().await)
}

/// One validated refresh with shared services and a stable repository selection.
///
/// Stages run in dependency order and retain independent reports. A failed stage does not erase
/// acquired source evidence or prevent another explicitly selected stage from reporting its result.
struct RefreshExecution<'a> {
    /// Already opened archive shared by acquisition and independently committed derived stages.
    archive: &'a Archive,
    /// Host-specific clients used only by the selected acquisition stage.
    github_clients: &'a std::collections::HashMap<GitHubHost, GitHubClient>,
    /// Optional vector service; absence becomes a selected-stage failure rather than scope
    /// failure.
    embedding_client: Option<&'a EmbeddingClient>,
    /// Validated stage selection and policy, retaining caller input separately from deduplicated
    /// scope.
    request: &'a RefreshRequest,
    /// Caller interruption forwarded to every stage; earlier durable work is retained.
    cancellation: &'a CancellationToken,
    /// Acquisition progress destination; derived stages report through their terminal reports.
    progress: Option<mpsc::Sender<SyncProgress>>,
    /// First-occurrence repository order shared by all stages after URL-based deduplication.
    repositories: Vec<RepositorySelector>,
}

impl RefreshExecution<'_> {
    /// Runs acquisition before derived analysis and computes the combined outcome last.
    async fn run(&self) -> RefreshReport {
        let mut report = RefreshReport {
            selected: self.selected(),
            sync: self.sync().await,
            embeddings: self.embeddings().await,
            clusters: self.clusters().await,
            remaining: Vec::new(),
            outcome: OperationOutcome::Complete,
        };
        report.remaining = remaining_stages(&report);
        report.outcome = refresh_outcome(&report);
        report
    }

    /// Lists selected stages in their execution order, regardless of caller selection order.
    fn selected(&self) -> Vec<RefreshStageKind> {
        let mut selected = Vec::with_capacity(1 + self.request.analysis.len());
        if self.request.sync.is_some() {
            selected.push(RefreshStageKind::Sync);
        }
        if self
            .request
            .analysis
            .contains(&RefreshAnalysisStage::Embeddings)
        {
            selected.push(RefreshStageKind::Embeddings);
        }
        if self
            .request
            .analysis
            .contains(&RefreshAnalysisStage::Clusters)
        {
            selected.push(RefreshStageKind::Clusters);
        }
        selected
    }

    /// Runs the selected sync stage and retains its partial result.
    async fn sync(&self) -> Option<RefreshStage<SyncReport>> {
        if let Some(options) = self.request.sync {
            let sync_request = SyncRequest {
                repositories: self.repositories.clone(),
                all: false,
                scope: options.scope,
                include_comments: options.include_comments,
                include_reviews: options.include_reviews,
                include_review_threads: options.include_review_threads,
                parent_run: None,
            };
            let stage = match sync_repositories(
                self.archive,
                self.github_clients,
                &sync_request,
                self.cancellation,
                self.progress.clone(),
            )
            .await
            {
                Ok(report) => stage_from_sync_report(report),
                Err(error) => RefreshStage::failed(stage_failure(&error)),
            };
            Some(stage)
        } else {
            None
        }
    }

    /// Runs the selected embeddings stage and retains its partial result.
    async fn embeddings(&self) -> Option<RefreshStage<RefreshEmbeddingReport>> {
        if self
            .request
            .analysis
            .contains(&RefreshAnalysisStage::Embeddings)
        {
            Some(match self.embedding_client {
                Some(client) => {
                    embed_repositories(
                        self.archive,
                        &self.repositories,
                        client,
                        self.request.recipe,
                        EmbeddingPolicy::from_force(self.request.force_embeddings),
                        self.cancellation,
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
        }
    }

    /// Runs the selected clusters stage and retains its partial result.
    async fn clusters(&self) -> Option<RefreshStage<Vec<super::RefreshClusterRepository>>> {
        if self
            .request
            .analysis
            .contains(&RefreshAnalysisStage::Clusters)
        {
            Some(
                build_repository_clusters(
                    self.archive,
                    &self.repositories,
                    self.request.embedding_identity.as_ref(),
                    self.request.recipe,
                    self.request.cluster_options,
                    self.cancellation,
                )
                .await,
            )
        } else {
            None
        }
    }
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
