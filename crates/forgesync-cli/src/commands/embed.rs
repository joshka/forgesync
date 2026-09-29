//! Embed command handling.

use super::*;

pub(super) struct EmbedCommandRequest<'a> {
    pub(super) archive_path: &'a std::path::Path,
    pub(super) repositories: Vec<RepositorySelector>,
    pub(super) service: crate::config::EmbeddingServiceConfig,
    pub(super) recipe: DocumentRecipe,
    pub(super) force: bool,
    pub(super) json: bool,
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
    if verbose > 0 && !json {
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
