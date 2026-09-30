//! # Run cluster generation from CLI options
//!
//! This module converts build arguments into an engine `ClusterBuildRequest`, opens the archive
//! for the required write, and renders the build report. Candidate scoring and grouping remain in
//! the engine.
//!
//! Keeping this flow separate from read and decision commands makes its derived-data side effects
//! visible. `ClusterBuildArgs` owns override conversion and the process execution boundary.
//! Preparation validates service identity without reading credentials or sending model requests;
//! clustering consumes compatible vectors already in the archive. Engine validation owns graph
//! policy bounds. The command closes the archive before presenting complete, partial, or failed
//! outcomes, and nearby tests establish request conversion without provider or archive setup.

use std::path::Path;
use std::process::ExitCode;

use forgesync_core::document::DocumentRecipe;
use forgesync_engine::clustering::{
    ClusterBuildReport, ClusterBuildRequest, ClusterOptions, build_clusters,
};
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;

use crate::command::cluster::ClusterBuildArgs;
use crate::config::{ConfigError, EmbeddingServiceConfig};
use crate::reports::clusters::cluster_build_summary;
use crate::{
    OutputMode, render_engine_error, render_error_with_status, render_result, render_store_error,
};

impl ClusterBuildArgs {
    /// Prepares local vector identity, executes the build, and closes before presentation.
    /// Configuration failures are usage errors and occur before opening the archive.
    pub async fn run_build(
        self,
        archive_path: &Path,
        embedding_service: EmbeddingServiceConfig,
        recipe: DocumentRecipe,
        json: OutputMode,
        verbose: u8,
        cancellation: &CancellationToken,
    ) -> ExitCode {
        let request = match self.prepare(embedding_service, recipe) {
            Ok(request) => request,
            Err(error) => return render_configuration_error(json, error),
        };
        log_request(&request, json, verbose);
        let archive = match Archive::open_read_write(archive_path).await {
            Ok(archive) => archive,
            Err(error) => return render_store_error(json, "cluster build", error),
        };
        let result = build_clusters(&archive, &request, cancellation).await;
        archive.close().await;
        match result {
            Ok(report) => render_report(json, report),
            Err(error) => render_engine_error(json, "cluster build", error),
        }
    }

    /// Applies identity overrides and builds the engine request without resolving credentials.
    /// Endpoint validation retains the application's secure/local service policy, even though
    /// clustering reads stored vectors and never sends a model request.
    fn prepare(
        self,
        mut service: EmbeddingServiceConfig,
        recipe: DocumentRecipe,
    ) -> Result<ClusterBuildRequest, ConfigError> {
        let options = self.options();
        if let Some(endpoint) = self.endpoint {
            service.endpoint = endpoint;
        }
        if let Some(model) = self.model {
            service.model = model;
        }
        service.validate()?;
        let endpoint =
            url::Url::parse(&service.endpoint).map_err(|_| ConfigError::InvalidEmbeddings)?;
        Ok(ClusterBuildRequest {
            repository: self.repository,
            endpoint: endpoint.as_str().trim_end_matches('/').to_owned(),
            model: service.model.trim().to_owned(),
            recipe,
            options,
        })
    }

    /// Converts parsed graph bounds without silently truncating values on narrower platforms.
    /// Engine validation remains responsible for rejecting invalid policy combinations.
    fn options(&self) -> ClusterOptions {
        ClusterOptions {
            threshold: self.threshold,
            cross_kind_threshold: self.cross_kind_threshold,
            fanout: usize::try_from(self.fanout).unwrap_or(usize::MAX),
            max_cluster_size: usize::try_from(self.max_cluster_size).unwrap_or(usize::MAX),
            min_cluster_size: usize::try_from(self.min_cluster_size).unwrap_or(usize::MAX),
        }
    }
}

/// Presents an invalid service configuration with the process usage-error status.
fn render_configuration_error(json: OutputMode, error: ConfigError) -> ExitCode {
    render_error_with_status(
        json,
        "cluster build",
        error.code(),
        &error.to_string(),
        ExitCode::from(2),
    )
}

/// Emits advisory build identity only when human-readable verbose output is requested.
fn log_request(request: &ClusterBuildRequest, json: OutputMode, verbose: u8) {
    if verbose > 0 && !json.is_json() {
        eprintln!(
            "forgesync: building local clusters for {} using stored vectors",
            request.repository.as_url()
        );
    }
}

/// Presents durable generation coverage, distinguishing partial work from complete success.
fn render_report(json: OutputMode, report: ClusterBuildReport) -> ExitCode {
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

#[cfg(test)]
#[path = "build_tests.rs"]
mod tests;
