//! Combine sync with explicitly requested analysis stages.
//!
//! A stage can fail after earlier evidence has been committed; the engine's structured stage
//! report shows what succeeded and what remains. An unusable embedding configuration does not fail
//! the command up front: the selected stage reports the missing capability instead.

use std::collections::HashMap;
use std::path::Path;

use clap::{ArgAction, Args};
use forgesync_core::identity::GitHubHost;
use forgesync_engine::clustering::ClusterOptions;
use forgesync_engine::embedding_client::EmbeddingClient;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::refresh::{RefreshAnalysisStage, RefreshRequest, refresh};
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;

use super::github::github_clients_for_selectors;
use super::sync::SyncScopeArgs;
use super::with_archive;
use crate::command::values::RefreshAnalysisArg;
use crate::config::ForgesyncConfig;
use crate::error::{CliError, Exit};
use crate::output::Output;
use crate::reports::sync::{outcome_exit_code, refresh_summary};

/// Sync a repository and run explicitly selected local analysis stages.
#[derive(Clone, Debug, Args)]
pub struct RefreshArgs {
    /// Repository scope shared by sync, embedding, and clustering stages.
    #[arg(value_name = "OWNER/REPO", required = true)]
    pub repositories: Vec<RepositorySelector>,
    /// Skip GitHub acquisition and analyze only the local archive.
    #[arg(
        long,
        action = ArgAction::SetTrue,
        requires = "analyze",
        conflicts_with_all = ["state", "with"]
    )]
    pub no_sync: bool,
    #[command(flatten)]
    pub scope: SyncScopeArgs,
    /// Explicitly select model-backed stages; clustering uses stored vectors.
    #[arg(long, value_enum, value_delimiter = ',')]
    pub analyze: Vec<RefreshAnalysisArg>,
    /// Force embedding requests even when compatible vectors are stored.
    #[arg(long, action = ArgAction::SetTrue)]
    pub force: bool,
}

impl RefreshArgs {
    pub async fn run(
        self,
        path: &Path,
        output: Output,
        verbose: u8,
        config: ForgesyncConfig,
        cancellation: &CancellationToken,
    ) -> Result<Exit, CliError> {
        self.validate()?;
        let report = with_archive(Archive::open_read_write(path), async |archive| {
            let clients = self.clients(verbose, cancellation).await?;
            let prepared = self.prepare(config);
            if verbose > 0 && !output.is_json() {
                eprintln!(
                    "forgesync: refreshing {}",
                    prepared.request.repositories.len()
                );
            }
            let report = refresh(
                archive,
                &clients,
                prepared.embedding_client.as_ref(),
                &prepared.request,
                cancellation,
                None,
            )
            .await;
            Ok::<_, CliError>(report?)
        })
        .await?;
        Ok(output.report(&report, refresh_summary, outcome_exit_code(&report.outcome)))
    }

    /// Skips credential discovery when there is no GitHub sync stage.
    async fn clients(
        &self,
        verbose: u8,
        cancellation: &CancellationToken,
    ) -> Result<HashMap<GitHubHost, GitHubClient>, CliError> {
        if self.no_sync {
            return Ok(HashMap::new());
        }
        github_clients_for_selectors(&self.repositories, verbose, cancellation).await
    }

    /// Builds the engine request with only the service capabilities its stages need.
    fn prepare(self, config: ForgesyncConfig) -> PreparedRefresh {
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
            .then(|| config.embeddings.client().ok())
            .flatten();
        let embedding_identity = analysis
            .contains(&RefreshAnalysisStage::Clusters)
            .then(|| config.embeddings.identity().ok())
            .flatten();
        let request = RefreshRequest {
            repositories: self.repositories,
            sync: (!self.no_sync).then(|| self.scope.options()),
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

    /// Rejects repeated analysis stages, which Clap cannot express.
    fn validate(&self) -> Result<(), CliError> {
        let repeated = self
            .analyze
            .iter()
            .enumerate()
            .any(|(index, stage)| self.analyze[..index].contains(stage));
        if repeated {
            return Err(CliError::Usage(
                "refresh analysis stages must be selected only once",
            ));
        }
        Ok(())
    }
}

/// Engine request plus the embedding client prepared for its selected stages, if usable.
struct PreparedRefresh {
    request: RefreshRequest,
    embedding_client: Option<EmbeddingClient>,
}

#[cfg(test)]
#[path = "refresh_tests.rs"]
mod tests;
