//! # Run cluster generation from CLI options
//!
//! This module converts build arguments into an engine `ClusterBuildRequest`, opens the archive
//! for the required write, and renders the build report. Candidate scoring and grouping remain in
//! the engine.
//!
//! Keeping this flow separate from read and decision commands makes its derived-data side effects
//! visible. A failed build should be reported from its structured outcome rather than treated as
//! an empty cluster list.

use std::path::Path;
use std::process::ExitCode;

use forgesync_core::document::DocumentRecipe;
use forgesync_engine::clustering::{ClusterBuildRequest, ClusterOptions, build_clusters};
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;

use crate::command::cluster::ClusterBuildArgs;
use crate::config::EmbeddingServiceConfig;
use crate::reports::cluster_build_summary;
use crate::{
    OutputMode, render_engine_error, render_error_with_status, render_result, render_store_error,
};

/// Builds one deterministic cluster generation using the selected model and policy overrides.
///
/// The archive is closed before rendering either the generated report or a failure.
pub async fn run_build(
    args: ClusterBuildArgs,
    archive_path: &Path,
    mut embedding_service: EmbeddingServiceConfig,
    recipe: DocumentRecipe,
    json: OutputMode,
    verbose: u8,
    cancellation: &CancellationToken,
) -> ExitCode {
    let ClusterBuildArgs {
        repository,
        endpoint,
        model,
        threshold,
        cross_kind_threshold,
        fanout,
        max_cluster_size,
        min_cluster_size,
    } = args;
    if let Some(endpoint) = endpoint {
        embedding_service.endpoint = endpoint;
    }
    if let Some(model) = model {
        embedding_service.model = model;
    }
    if let Err(error) = embedding_service.validate() {
        return render_error_with_status(
            json,
            "cluster build",
            error.code(),
            &error.to_string(),
            ExitCode::from(2),
        );
    }
    let endpoint = match url::Url::parse(&embedding_service.endpoint) {
        Ok(endpoint) => endpoint.as_str().trim_end_matches('/').to_owned(),
        Err(_) => {
            return render_error_with_status(
                json,
                "cluster build",
                "embedding_configuration_invalid",
                "embedding service configuration is invalid",
                ExitCode::from(2),
            );
        }
    };
    let request = ClusterBuildRequest {
        repository,
        endpoint,
        model: embedding_service.model.trim().to_owned(),
        recipe,
        options: ClusterOptions {
            threshold,
            cross_kind_threshold,
            fanout: usize::try_from(fanout).unwrap_or(usize::MAX),
            max_cluster_size: usize::try_from(max_cluster_size).unwrap_or(usize::MAX),
            min_cluster_size: usize::try_from(min_cluster_size).unwrap_or(usize::MAX),
        },
    };
    if verbose > 0 && !json.is_json() {
        eprintln!(
            "forgesync: building local clusters for {} using stored vectors",
            request.repository.as_url()
        );
    }
    let archive = match Archive::open_read_write(archive_path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(json, "cluster build", error),
    };
    let result = build_clusters(&archive, &request, cancellation).await;
    archive.close().await;
    match result {
        Ok(report) => {
            let status = if report.generation.complete_coverage {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(3)
            };
            render_result(
                json,
                "cluster build",
                &report,
                cluster_build_summary,
                status,
            )
        }
        Err(error) => render_engine_error(json, "cluster build", error),
    }
}
