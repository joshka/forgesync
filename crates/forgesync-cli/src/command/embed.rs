//! # Generate embeddings on explicit request
//!
//! `EmbedArgs` carries repository scope and service settings for a derived-data operation. Its run
//! method resolves the configured client, asks the engine to embed eligible documents, and renders
//! completed and failed batches.
//!
//! Embedding service calls are distinct from GitHub acquisition. A local search reads stored
//! vectors; this command is where a user chooses to produce new ones.

use std::collections::HashSet;
use std::process::ExitCode;

use clap::{ArgAction, Args};
use forgesync_engine::embedding_client::EmbeddingClient;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::refresh::{RefreshStageFailure, RefreshStageStatus, embed_repositories};
use forgesync_store::archive::Archive;

use crate::config::ForgesyncConfig;
use crate::reports::{EmbeddingOutput, embedding_summary};
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
        let cancellation = tokio_util::sync::CancellationToken::new();
        let interrupt_cancellation = cancellation.clone();
        let interrupt_task = tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                interrupt_cancellation.cancel();
            }
        });
        let result = execute_embed(self, path, json, verbose, config, &cancellation).await;
        interrupt_task.abort();
        result
    }
}

/// Executes one prepared embed request against the selected archive.
async fn execute_embed(
    args: EmbedArgs,
    path: &std::path::Path,
    json: OutputMode,
    verbose: u8,
    config: ForgesyncConfig,
    cancellation: &tokio_util::sync::CancellationToken,
) -> ExitCode {
    let EmbedArgs {
        repositories,
        force,
        endpoint,
        model,
        api_key_env,
        dimensions,
        max_input_bytes,
        max_batch_input_bytes,
        batch_size,
        concurrency,
    } = args;
    let mut service = config.embeddings;
    let recipe = config.documents.recipe;
    if let Some(endpoint) = endpoint {
        service.endpoint = endpoint;
    }
    if let Some(model) = model {
        service.model = model;
    }
    if let Some(api_key_env) = api_key_env {
        service.api_key_env = api_key_env;
    }
    if let Some(dimensions) = dimensions {
        service.dimensions = Some(dimensions);
    }
    if let Some(max_input_bytes) = max_input_bytes {
        service.max_input_bytes = max_input_bytes as usize;
    }
    if let Some(max_batch_input_bytes) = max_batch_input_bytes {
        service.max_batch_input_bytes = max_batch_input_bytes as usize;
    }
    if let Some(batch_size) = batch_size {
        service.batch_size = batch_size as usize;
    }
    if let Some(concurrency) = concurrency {
        service.concurrency = concurrency as usize;
    }
    if let Err(error) = service.validate() {
        return render_error_with_status(
            json,
            "embed",
            error.code(),
            &error.to_string(),
            ExitCode::from(2),
        );
    }
    let api_key = std::env::var(&service.api_key_env).unwrap_or_default();
    let client_config = match service.client_config(api_key) {
        Ok(config) => config,
        Err(error) => {
            return render_error_with_status(
                json,
                "embed",
                error.code(),
                &error.to_string(),
                ExitCode::from(2),
            );
        }
    };
    let client = match EmbeddingClient::new(client_config) {
        Ok(client) => client,
        Err(error) => {
            return render_error_with_status(
                json,
                "embed",
                error.code(),
                &error.to_string(),
                ExitCode::from(2),
            );
        }
    };
    let archive = match Archive::open_read_write(path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(json, "embed", error),
    };
    let mut repositories = repositories
        .into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    repositories.sort_by_key(RepositorySelector::as_url);
    if verbose > 0 && !json.is_json() {
        eprintln!(
            "forgesync: embedding discussions in {} repository(s)",
            repositories.len()
        );
    }
    let stage = embed_repositories(
        &archive,
        &repositories,
        &client,
        recipe,
        force,
        cancellation,
    )
    .await;
    archive.close().await;
    let Some(stage_report) = stage.report else {
        let failure = stage.failure.unwrap_or(RefreshStageFailure {
            code: "embedding_stage_failed",
            message: "embedding stage did not produce a report".to_owned(),
        });
        let status = if failure.code == "operation_cancelled" {
            ExitCode::from(130)
        } else {
            ExitCode::FAILURE
        };
        return render_error_with_status(json, "embed", failure.code, &failure.message, status);
    };

    let output = EmbeddingOutput {
        repositories: repositories
            .iter()
            .map(RepositorySelector::as_url)
            .collect(),
        recipe,
        endpoint: client.endpoint_identity().to_owned(),
        model: client.model().to_owned(),
        dimensions: service.dimensions,
        status: stage.status,
        report: stage_report.embeddings,
        documents_materialized: stage_report.documents_materialized,
        document_failures: stage_report.document_failures,
        failure: stage.failure,
    };
    let exit_status = match output.status {
        RefreshStageStatus::Complete => ExitCode::SUCCESS,
        RefreshStageStatus::Partial | RefreshStageStatus::Deferred => ExitCode::from(3),
        RefreshStageStatus::Interrupted => ExitCode::from(130),
        RefreshStageStatus::Failed => ExitCode::FAILURE,
    };
    render_result(json, "embed", &output, embedding_summary, exit_status)
}
