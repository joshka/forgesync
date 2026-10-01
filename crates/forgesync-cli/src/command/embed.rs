//! Generate embeddings on explicit request.
//!
//! This is where a user chooses to send discussion text to the embedding service; search and
//! clustering read stored vectors.

use std::collections::HashSet;
use std::path::Path;

use clap::builder::RangedU64ValueParser;
use clap::{ArgAction, Args};
use forgesync_core::document::DocumentRecipe;
use forgesync_engine::embedding_client::EmbeddingClient;
use forgesync_engine::embeddings::EmbeddingPolicy;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::refresh::{
    RefreshEmbeddingReport, RefreshStage, RefreshStageFailure, embed_repositories,
};
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;

use super::with_archive;
use crate::config::{EmbeddingServiceConfig, ForgesyncConfig};
use crate::error::{CliError, Exit};
use crate::output::Output;
use crate::reports::embedding::EmbeddingOutput;

/// Build documents and store compatible embeddings for local discussions.
#[derive(Clone, Debug, Args)]
pub struct EmbedArgs {
    /// One or more registered repositories to embed.
    #[arg(value_name = "OWNER/REPO", required = true)]
    pub repositories: Vec<RepositorySelector>,
    /// Force provider requests even when current compatible vectors are stored.
    #[arg(long, action = ArgAction::SetTrue)]
    pub force: bool,
    #[command(flatten)]
    pub service: EmbeddingOverrides,
}

/// Endpoint and model overrides that identify an embedding service and its stored vectors.
#[derive(Clone, Debug, Default, Args)]
pub struct EmbeddingIdentityArgs {
    /// Override the configured OpenAI-compatible base endpoint.
    #[arg(long, value_name = "URL")]
    pub endpoint: Option<String>,
    /// Override the configured embedding model.
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,
}

impl EmbeddingIdentityArgs {
    /// Replaces the configured endpoint and model with any supplied values.
    pub fn apply(self, service: &mut EmbeddingServiceConfig) {
        if let Some(endpoint) = self.endpoint {
            service.endpoint = endpoint;
        }
        if let Some(model) = self.model {
            service.model = model;
        }
    }
}

/// Command-line overrides applied after file configuration.
#[derive(Clone, Debug, Default, Args)]
pub struct EmbeddingOverrides {
    #[command(flatten)]
    pub identity: EmbeddingIdentityArgs,
    /// Override the environment variable name containing the API key.
    #[arg(long, value_name = "NAME")]
    pub api_key_env: Option<String>,
    /// Override the expected output dimensions.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=65536))]
    pub dimensions: Option<u32>,
    /// Override the maximum UTF-8 bytes per input chunk.
    #[arg(long, value_parser = RangedU64ValueParser::<usize>::new().range(4..=300_000))]
    pub max_input_bytes: Option<usize>,
    /// Override the maximum UTF-8 bytes in one request.
    #[arg(long, value_parser = RangedU64ValueParser::<usize>::new().range(4..=300_000))]
    pub max_batch_input_bytes: Option<usize>,
    /// Override the maximum inputs per request.
    #[arg(long, value_parser = RangedU64ValueParser::<usize>::new().range(1..=2048))]
    pub batch_size: Option<usize>,
    /// Override the maximum requests in flight for this service.
    #[arg(long, value_parser = RangedU64ValueParser::<usize>::new().range(1..=64))]
    pub concurrency: Option<usize>,
}

impl EmbeddingOverrides {
    /// Replaces configured settings with any values supplied on the command line.
    pub fn apply(self, service: &mut EmbeddingServiceConfig) {
        self.identity.apply(service);
        if let Some(api_key_env) = self.api_key_env {
            service.api_key_env = api_key_env;
        }
        service.dimensions = self.dimensions.or(service.dimensions);
        service.max_input_bytes = self.max_input_bytes.unwrap_or(service.max_input_bytes);
        service.max_batch_input_bytes = self
            .max_batch_input_bytes
            .unwrap_or(service.max_batch_input_bytes);
        service.batch_size = self.batch_size.unwrap_or(service.batch_size);
        service.concurrency = self.concurrency.unwrap_or(service.concurrency);
    }
}

impl EmbedArgs {
    /// Embeds the selected repositories into a writable archive and renders the stage report.
    pub async fn run(
        self,
        path: &Path,
        output: Output,
        verbose: u8,
        config: ForgesyncConfig,
        cancellation: &CancellationToken,
    ) -> Result<Exit, CliError> {
        let prepared = self.prepare(config)?;
        let stage = with_archive(Archive::open_read_write(path), async |archive| {
            if verbose > 0 && !output.is_json() {
                eprintln!(
                    "forgesync: embedding discussions in {} repository(s)",
                    prepared.repositories.len()
                );
            }
            Ok::<_, CliError>(
                embed_repositories(
                    archive,
                    &prepared.repositories,
                    &prepared.client,
                    prepared.recipe,
                    prepared.policy,
                    cancellation,
                )
                .await,
            )
        })
        .await?;
        let report = prepared.output(stage)?;
        Ok(output.report(&report, EmbeddingOutput::summary, report.exit_status()))
    }

    /// Resolves overrides and the client before any archive is opened.
    fn prepare(self, config: ForgesyncConfig) -> Result<PreparedEmbedding, CliError> {
        let mut service = config.embeddings;
        self.service.apply(&mut service);
        Ok(PreparedEmbedding {
            repositories: repository_scope(self.repositories),
            client: service.client()?,
            recipe: config.documents.recipe,
            policy: if self.force {
                EmbeddingPolicy::Replace
            } else {
                EmbeddingPolicy::Missing
            },
            dimensions: service.dimensions,
        })
    }
}

/// Deduplicates selectors and orders them by canonical URL for repeatable execution and output.
fn repository_scope(repositories: Vec<RepositorySelector>) -> Vec<RepositorySelector> {
    let mut repositories = repositories
        .into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    repositories.sort_by_key(RepositorySelector::as_url);
    repositories
}

/// Resolved scope and client, prepared before the archive is opened.
struct PreparedEmbedding {
    repositories: Vec<RepositorySelector>,
    client: EmbeddingClient,
    recipe: DocumentRecipe,
    policy: EmbeddingPolicy,
    dimensions: Option<u32>,
}

impl PreparedEmbedding {
    /// Attaches the execution identity to the stage report; a missing report is an error.
    fn output(
        self,
        stage: RefreshStage<RefreshEmbeddingReport>,
    ) -> Result<EmbeddingOutput, CliError> {
        let Some(report) = stage.report else {
            return Err(CliError::Stage {
                status: stage.status,
                failure: stage.failure.unwrap_or(RefreshStageFailure {
                    code: "embedding_stage_failed",
                    message: "embedding stage did not produce a report".to_owned(),
                }),
            });
        };
        Ok(EmbeddingOutput {
            repositories: self
                .repositories
                .iter()
                .map(RepositorySelector::as_url)
                .collect(),
            recipe: self.recipe,
            endpoint: self.client.endpoint_identity().to_owned(),
            model: self.client.model().to_owned(),
            dimensions: self.dimensions,
            status: stage.status,
            report: report.embeddings,
            documents_materialized: report.documents_materialized,
            document_failures: report.document_failures,
            failure: stage.failure,
        })
    }
}

#[cfg(test)]
#[path = "embed_tests.rs"]
mod tests;
