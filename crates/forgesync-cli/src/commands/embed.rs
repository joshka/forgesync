//! Embed command handling.

use super::*;

pub(super) struct EmbedCommandRequest<'a> {
    pub(super) archive_path: &'a std::path::Path,
    pub(super) repositories: Vec<RepositorySelector>,
    pub(super) service: crate::config::EmbeddingServiceConfig,
    pub(super) recipe: DocumentRecipe,
    pub(super) force: bool,
    pub(super) json: OutputMode,
    pub(super) verbose: u8,
    pub(super) cancellation: &'a tokio_util::sync::CancellationToken,
}

pub(super) async fn embed_command(request: EmbedCommandRequest<'_>) -> ExitCode {
    let EmbedCommandRequest {
        archive_path,
        repositories,
        service,
        recipe,
        force,
        json,
        verbose,
        cancellation,
    } = request;
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
    let archive = match Archive::open_read_write(archive_path).await {
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

pub(super) struct EmbedCliRequest<'a> {
    pub(super) path: &'a std::path::Path,
    pub(super) service: crate::config::EmbeddingServiceConfig,
    pub(super) recipe: DocumentRecipe,
    pub(super) json: OutputMode,
    pub(super) verbose: u8,
    pub(super) repositories: Vec<RepositorySelector>,
    pub(super) force: bool,
    pub(super) endpoint: Option<String>,
    pub(super) model: Option<String>,
    pub(super) api_key_env: Option<String>,
    pub(super) dimensions: Option<u32>,
    pub(super) max_input_bytes: Option<u32>,
    pub(super) max_batch_input_bytes: Option<u32>,
    pub(super) batch_size: Option<u32>,
    pub(super) concurrency: Option<u32>,
}

pub(super) async fn embed_from_cli(request: EmbedCliRequest<'_>) -> ExitCode {
    let EmbedCliRequest {
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
        ..
    } = request;
    let cancellation = tokio_util::sync::CancellationToken::new();
    let interrupt_cancellation = cancellation.clone();
    let interrupt_task = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            interrupt_cancellation.cancel();
        }
    });
    let mut service = request.service;
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
    let result = embed_command(EmbedCommandRequest {
        archive_path: request.path,
        repositories,
        service,
        recipe: request.recipe,
        force,
        json: request.json,
        verbose: request.verbose,
        cancellation: &cancellation,
    })
    .await;
    interrupt_task.abort();
    result
}
