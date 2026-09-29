//! # Combine sync with requested analysis stages
//!
//! `RefreshArgs` describes acquisition scope and optional embedding or clustering stages. Embedding
//! analysis materializes its documents before model-backed work. Its run method builds the engine
//! refresh request, supplies clients where needed, and renders each stage's outcome.
//! `PreparedRefresh` keeps stage selection and its optional embedding capability together; it runs
//! against an archive opened and closed by the command boundary.
//!
//! A stage can fail after earlier evidence has been committed. The command preserves the engine's
//! structured stage report so the user can tell what succeeded and what remains to retry.
//! Credential discovery is skipped for local-only refresh. Setup and engine errors retain typed
//! causes through archive closure, then presentation applies the established output envelope and
//! exit policy.

use std::collections::HashMap;
use std::process::ExitCode;

use clap::{ArgAction, Args};
use forgesync_core::identity::GitHubHost;
use forgesync_engine::clustering::ClusterOptions;
use forgesync_engine::embedding_client::EmbeddingClient;
use forgesync_engine::error::EngineError;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::refresh::{
    EmbeddingServiceIdentity, RefreshAnalysisStage, RefreshReport, RefreshRequest,
    RefreshSyncOptions, refresh,
};
use forgesync_engine::sync::SyncThreadScope;
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;

use super::github::{
    GitHubClientSetupError, github_clients_for_selectors, render_github_client_setup_error,
};
use crate::command::values::{RefreshAnalysisArg, SyncIncludeArg, SyncThreadStateArg};
use crate::config::ForgesyncConfig;
use crate::reports::sync::{outcome_exit_code, refresh_summary};
use crate::{OutputMode, render_engine_error, render_result, render_store_error, usage_error};

/// Sync a repository and run explicitly selected local analysis stages.
#[derive(Clone, Debug, Args)]
pub struct RefreshArgs {
    /// Repository scope shared by sync, embedding, and clustering stages.
    #[arg(value_name = "OWNER/REPO", required = true)]
    pub repositories: Vec<RepositorySelector>,
    /// Skip GitHub acquisition and analyze only the local archive.
    #[arg(long, action = ArgAction::SetTrue)]
    pub no_sync: bool,
    /// Select open threads, closed threads, or a complete all-state enumeration.
    #[arg(long, value_enum)]
    pub state: Option<SyncThreadStateArg>,
    /// Add selected evidence families to the sync stage.
    #[arg(long = "with", value_enum, value_delimiter = ',')]
    pub with: Vec<SyncIncludeArg>,
    /// Explicitly select model-backed stages; clustering uses stored vectors.
    #[arg(long, value_enum, value_delimiter = ',')]
    pub analyze: Vec<RefreshAnalysisArg>,
    /// Force embedding requests even when compatible vectors are stored.
    #[arg(long, action = ArgAction::SetTrue)]
    pub force: bool,
}

impl RefreshArgs {
    /// Runs the selected refresh workflow with process cancellation and result rendering.
    pub async fn run(
        self,
        path: &std::path::Path,
        json: OutputMode,
        verbose: u8,
        config: ForgesyncConfig,
    ) -> ExitCode {
        let interruption = super::interruption::CommandInterruption::new();
        let cancellation = interruption.cancellation();
        self.execute(path, json, verbose, config, cancellation)
            .await
    }

    /// Owns the archive lifetime across capability preparation and selected stage execution.
    async fn execute(
        self,
        path: &std::path::Path,
        json: OutputMode,
        verbose: u8,
        config: ForgesyncConfig,
        cancellation: &tokio_util::sync::CancellationToken,
    ) -> ExitCode {
        if let Err(message) = self.validate() {
            return usage_error(message);
        }
        let archive = match Archive::open_read_write(path).await {
            Ok(archive) => archive,
            Err(error) => return render_store_error(json, "refresh", error),
        };
        let result = self
            .acquire(&archive, json, verbose, config, cancellation)
            .await;
        archive.close().await;
        match result {
            Ok(report) => render_report(json, report),
            Err(error) => error.render(json),
        }
    }

    /// Prepares only selected provider/service capabilities, then acquires the requested stages.
    /// Optional embedding configuration remains stage-local so earlier sync work can still succeed.
    async fn acquire(
        self,
        archive: &Archive,
        output: OutputMode,
        verbose: u8,
        config: ForgesyncConfig,
        cancellation: &CancellationToken,
    ) -> Result<RefreshReport, RefreshFailure> {
        let clients = self.clients(verbose, cancellation).await?;
        let prepared = self.prepare(config);
        if verbose > 0 && !output.is_json() {
            eprintln!(
                "forgesync: refreshing {}",
                prepared.request.repositories.len()
            );
        }
        prepared
            .run(archive, &clients, cancellation)
            .await
            .map_err(RefreshFailure::Engine)
    }

    /// Skips credential discovery entirely when the selected workflow has no GitHub sync stage.
    async fn clients(
        &self,
        verbose: u8,
        cancellation: &CancellationToken,
    ) -> Result<HashMap<GitHubHost, GitHubClient>, RefreshFailure> {
        if self.no_sync {
            return Ok(HashMap::new());
        }
        github_clients_for_selectors(&self.repositories, verbose, cancellation)
            .await
            .map_err(RefreshFailure::ClientSetup)
    }

    /// Keeps engine stage selection and its optional service capability in one prepared value.
    /// Configuration/client failures remain absent capabilities, reported by the selected stage.
    fn prepare(self, config: ForgesyncConfig) -> PreparedRefresh {
        let sync = self.sync_options();
        let analysis = self
            .analyze
            .into_iter()
            .map(|stage| match stage {
                RefreshAnalysisArg::Embeddings => RefreshAnalysisStage::Embeddings,
                RefreshAnalysisArg::Clusters => RefreshAnalysisStage::Clusters,
            })
            .collect::<Vec<_>>();
        let embedding_client = analysis
            .contains(&RefreshAnalysisStage::Embeddings)
            .then(|| optional_embedding_client(&config.embeddings))
            .flatten();
        let embedding_identity = analysis
            .contains(&RefreshAnalysisStage::Clusters)
            .then(|| configured_embedding_identity(&config.embeddings))
            .flatten();
        let request = RefreshRequest {
            repositories: self.repositories,
            sync,
            analysis,
            recipe: config.documents.recipe,
            embedding_identity,
            force_embeddings: self.force,
            cluster_options: ClusterOptions::default(),
        };
        PreparedRefresh {
            request,
            embedding_client,
        }
    }

    /// Converts parsed state/family choices into the optional engine acquisition scope.
    fn sync_options(&self) -> Option<RefreshSyncOptions> {
        if self.no_sync {
            return None;
        }
        Some(RefreshSyncOptions {
            scope: match self.state {
                None => SyncThreadScope::Default,
                Some(SyncThreadStateArg::Open) => SyncThreadScope::Open,
                Some(SyncThreadStateArg::Closed) => SyncThreadScope::Closed,
                Some(SyncThreadStateArg::All) => SyncThreadScope::All,
            },
            include_comments: self.with.contains(&SyncIncludeArg::Comments),
            include_reviews: self.with.contains(&SyncIncludeArg::Reviews),
            include_review_threads: self.with.contains(&SyncIncludeArg::ReviewThreads),
        })
    }

    /// Rejects inconsistent stage selections before opening an archive or resolving credentials.
    fn validate(&self) -> Result<(), &'static str> {
        if self.no_sync && self.analyze.is_empty() {
            return Err("refresh requires sync or at least one --analyze stage");
        }
        if self
            .analyze
            .iter()
            .enumerate()
            .any(|(index, stage)| self.analyze[..index].contains(stage))
        {
            return Err("refresh analysis stages must be selected only once");
        }
        if self.no_sync && (self.state.is_some() || !self.with.is_empty()) {
            return Err("--state and --with require the refresh sync stage");
        }
        Ok(())
    }
}

/// Engine request and the model-service client prepared specifically for its selected stages.
///
/// An absent client is intentional for cluster-only analysis and preserves structured stage failure
/// when embeddings are selected but optional service configuration is unusable.
struct PreparedRefresh {
    /// Repository scope, selected ordered stages, and local analysis policy.
    request: RefreshRequest,
    /// Optional embedding capability; cluster-only work reads stored vectors without one.
    embedding_client: Option<EmbeddingClient>,
}

impl PreparedRefresh {
    /// Executes the selected stages against an already opened archive and explicit cancellation.
    async fn run(
        self,
        archive: &Archive,
        clients: &HashMap<GitHubHost, GitHubClient>,
        cancellation: &CancellationToken,
    ) -> Result<RefreshReport, EngineError> {
        refresh(
            archive,
            clients,
            self.embedding_client.as_ref(),
            &self.request,
            cancellation,
            None,
        )
        .await
    }
}

/// Renders the structured stage report after archive closure, retaining aggregate outcome policy.
fn render_report(output: OutputMode, report: RefreshReport) -> ExitCode {
    let exit_status = outcome_exit_code(&report.outcome);
    render_result(output, "refresh", &report, refresh_summary, exit_status)
}

/// Typed workflow boundary failure retained until the archive has been closed.
#[derive(Debug, thiserror::Error)]
enum RefreshFailure {
    /// Credential discovery or provider transport setup failed before acquisition.
    #[error("provider client setup failed: {0}")]
    ClientSetup(#[source] GitHubClientSetupError),
    /// The engine could not return a structured stage report.
    #[error("refresh failed: {0}")]
    Engine(#[source] EngineError),
}

impl RefreshFailure {
    /// Preserves the established safe process envelope and exit policy after resource cleanup.
    fn render(self, output: OutputMode) -> ExitCode {
        match self {
            Self::ClientSetup(error) => render_github_client_setup_error(output, "refresh", error),
            Self::Engine(error) => render_engine_error(output, "refresh", error),
        }
    }
}

/// Builds an embedding client when the configured service is usable.
///
/// Invalid optional configuration returns `None` so a refresh without embedding analysis can
/// proceed. The selected embedding stage reports an unavailable service if it needs this client.
pub fn optional_embedding_client(
    service: &crate::config::EmbeddingServiceConfig,
) -> Option<EmbeddingClient> {
    service.client().ok()
}

/// Derives the stable endpoint and model identity used to select compatible stored vectors.
pub fn configured_embedding_identity(
    service: &crate::config::EmbeddingServiceConfig,
) -> Option<EmbeddingServiceIdentity> {
    service.validate().ok()?;
    let endpoint = url::Url::parse(&service.endpoint).ok()?;
    Some(EmbeddingServiceIdentity {
        endpoint: endpoint.as_str().trim_end_matches('/').to_owned(),
        model: service.model.trim().to_owned(),
    })
}

#[cfg(test)]
#[path = "refresh_tests.rs"]
mod tests;
