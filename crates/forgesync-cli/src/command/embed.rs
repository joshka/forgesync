//! # Generate embeddings on explicit request
//!
//! `EmbedArgs` carries repository scope and service settings for a derived-data operation. Its run
//! method resolves the configured client, asks the engine to embed eligible documents, and renders
//! completed and failed batches. `PreparedEmbedding` keeps the selected scope, service identity,
//! recipe, and typed replacement policy together through acquisition and output projection.
//! Configuration fails before an archive is opened; the command closes a successful open before
//! rendering either a report or a missing-report diagnostic.
//!
//! Embedding service calls are distinct from GitHub acquisition. A local search reads stored
//! vectors; this command is where a user chooses to produce new ones.

use std::collections::HashSet;
use std::process::ExitCode;

use clap::{ArgAction, Args};
use forgesync_core::document::DocumentRecipe;
use forgesync_engine::embedding_client::EmbeddingClient;
use forgesync_engine::embeddings::EmbeddingPolicy;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::refresh::{
    RefreshEmbeddingReport, RefreshStage, RefreshStageFailure, embed_repositories,
};
use forgesync_store::archive::Archive;

use super::embedding_service::EmbeddingSetupError;
use crate::config::ForgesyncConfig;
use crate::reports::embedding::EmbeddingOutput;
use crate::{OutputMode, render_error_with_status, render_result, render_store_error};

/// Build documents and store compatible embeddings for local discussions.
#[derive(Clone, Debug, Args)]
pub struct EmbedArgs {
    /// One or more registered repositories to embed.
    #[arg(value_name = "OWNER/REPO", required = true)]
    pub repositories: Vec<RepositorySelector>,
    /// Force provider requests even when current compatible vectors are stored.
    #[arg(long, action = ArgAction::SetTrue)]
    pub force: bool,
    /// Override the configured OpenAI-compatible base endpoint.
    #[arg(long, value_name = "URL")]
    pub endpoint: Option<String>,
    /// Override the configured embedding model.
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,
    /// Override the environment variable name containing the API key.
    #[arg(long, value_name = "NAME")]
    pub api_key_env: Option<String>,
    /// Override the expected output dimensions.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=65536))]
    pub dimensions: Option<u32>,
    /// Override the maximum UTF-8 bytes per input chunk.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=300000))]
    pub max_input_bytes: Option<u32>,
    /// Override the maximum UTF-8 bytes in one request.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=300000))]
    pub max_batch_input_bytes: Option<u32>,
    /// Override the maximum inputs per request.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=2048))]
    pub batch_size: Option<u32>,
    /// Override the maximum requests in flight for this service.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=64))]
    pub concurrency: Option<u32>,
}

impl EmbedArgs {
    /// Runs the selected embed workflow with process cancellation and result rendering.
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

    /// Executes one prepared embed request against the selected archive.
    async fn execute(
        self,
        path: &std::path::Path,
        json: OutputMode,
        verbose: u8,
        config: ForgesyncConfig,
        cancellation: &tokio_util::sync::CancellationToken,
    ) -> ExitCode {
        let prepared = match self.prepare(config) {
            Ok(prepared) => prepared,
            Err(error) => return render_configuration_error(json, error),
        };
        let archive = match Archive::open_read_write(path).await {
            Ok(archive) => archive,
            Err(error) => return render_store_error(json, "embed", error),
        };
        if verbose > 0 && !json.is_json() {
            eprintln!(
                "forgesync: embedding discussions in {} repository(s)",
                prepared.repositories.len()
            );
        }
        let result = prepared.run(&archive, cancellation).await;
        archive.close().await;
        match result {
            Ok(output) => render_report(json, output),
            Err(failure) => render_stage_failure(json, failure),
        }
    }

    /// Resolves overrides and credentials before opening an archive, then fixes execution scope.
    /// Repository deduplication and ordering are pure; model requests occur only in `run`.
    fn prepare(self, config: ForgesyncConfig) -> Result<PreparedEmbedding, EmbeddingSetupError> {
        let service = self.service(config.embeddings);
        let client = service.client()?;
        let repositories = repository_scope(self.repositories);
        let policy = if self.force {
            EmbeddingPolicy::Replace
        } else {
            EmbeddingPolicy::Missing
        };
        Ok(PreparedEmbedding {
            repositories,
            client,
            recipe: config.documents.recipe,
            policy,
            dimensions: service.dimensions,
        })
    }

    /// Applies command-line service overrides after file and environment config resolution.
    fn service(
        &self,
        mut service: crate::config::EmbeddingServiceConfig,
    ) -> crate::config::EmbeddingServiceConfig {
        if let Some(endpoint) = self.endpoint.clone() {
            service.endpoint = endpoint;
        }
        if let Some(model) = self.model.clone() {
            service.model = model;
        }
        if let Some(api_key_env) = self.api_key_env.clone() {
            service.api_key_env = api_key_env;
        }
        if let Some(dimensions) = self.dimensions {
            service.dimensions = Some(dimensions);
        }
        if let Some(max_input_bytes) = self.max_input_bytes {
            service.max_input_bytes = max_input_bytes as usize;
        }
        if let Some(max_batch_input_bytes) = self.max_batch_input_bytes {
            service.max_batch_input_bytes = max_batch_input_bytes as usize;
        }
        if let Some(batch_size) = self.batch_size {
            service.batch_size = batch_size as usize;
        }
        if let Some(concurrency) = self.concurrency {
            service.concurrency = concurrency as usize;
        }
        service
    }
}

/// Deduplicates selectors and orders their canonical URLs for repeatable execution and JSON scope.
/// This pure preparation does not resolve repositories in the archive or contact their hosts.
fn repository_scope(repositories: Vec<RepositorySelector>) -> Vec<RepositorySelector> {
    let mut repositories = repositories
        .into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    repositories.sort_by_key(RepositorySelector::as_url);
    repositories
}

/// Prepared repository selection and service capability used for acquisition and its output
/// identity.
struct PreparedEmbedding {
    /// Unique host-qualified selectors ordered by URL for repeatable execution and presentation.
    repositories: Vec<RepositorySelector>,
    /// Validated service client; its normalized identity also labels the resulting report.
    client: EmbeddingClient,
    /// Document materialization recipe shared with vector compatibility selection.
    recipe: DocumentRecipe,
    /// Typed cache/replacement policy converted once from the parsed command flag.
    policy: EmbeddingPolicy,
    /// Configured output dimensions attached to the same client/request identity.
    dimensions: Option<u32>,
}

impl PreparedEmbedding {
    /// Acquires documents/vectors and projects the result without rendering or closing the archive.
    async fn run(
        self,
        archive: &Archive,
        cancellation: &tokio_util::sync::CancellationToken,
    ) -> Result<EmbeddingOutput, RefreshStageFailure> {
        let stage = embed_repositories(
            archive,
            &self.repositories,
            &self.client,
            self.recipe,
            self.policy,
            cancellation,
        )
        .await;
        self.output(stage)
    }

    /// Attaches the exact execution identity to a present report, retaining safe partial failures.
    /// An absent report becomes a concrete diagnostic rather than an optional error value.
    fn output(
        self,
        stage: RefreshStage<RefreshEmbeddingReport>,
    ) -> Result<EmbeddingOutput, RefreshStageFailure> {
        let Some(report) = stage.report else {
            return Err(stage.failure.unwrap_or(RefreshStageFailure {
                code: "embedding_stage_failed",
                message: "embedding stage did not produce a report".to_owned(),
            }));
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

/// Presents invalid service configuration before archive creation or acquisition can begin.
fn render_configuration_error(output: OutputMode, error: EmbeddingSetupError) -> ExitCode {
    render_error_with_status(
        output,
        "embed",
        error.code(),
        &error.to_string(),
        ExitCode::from(2),
    )
}

/// Renders a report-bearing outcome after the command closes its writable archive.
fn render_report(output: OutputMode, report: EmbeddingOutput) -> ExitCode {
    let status = report.exit_status();
    render_result(output, "embed", &report, EmbeddingOutput::summary, status)
}

/// Presents a stage that produced no report after archive cleanup, preserving cancellation status.
fn render_stage_failure(output: OutputMode, failure: RefreshStageFailure) -> ExitCode {
    let status = failure_exit_status(&failure);
    render_error_with_status(output, "embed", failure.code, &failure.message, status)
}

/// Returns the established process status for a safe stage diagnostic without a report.
/// Cancellation is identified by its stable code; all other missing-report failures are fatal.
fn failure_exit_status(failure: &RefreshStageFailure) -> ExitCode {
    if failure.code == "operation_cancelled" {
        ExitCode::from(130)
    } else {
        ExitCode::FAILURE
    }
}

#[cfg(test)]
#[path = "embed_tests.rs"]
mod tests;
