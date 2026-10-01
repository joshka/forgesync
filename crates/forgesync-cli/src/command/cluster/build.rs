//! Cluster generation from stored vectors.
//!
//! Service identity selects compatible vectors already in the archive; no credentials are read
//! and no model request is sent. Graph policy bounds are validated by the engine.

use std::path::Path;

use forgesync_core::document::DocumentRecipe;
use forgesync_engine::clustering::{ClusterBuildRequest, ClusterOptions, build_clusters};
use forgesync_engine::refresh::EmbeddingServiceIdentity;
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;

use crate::command::cluster::ClusterBuildArgs;
use crate::command::with_archive;
use crate::config::{ConfigError, EmbeddingServiceConfig, ForgesyncConfig};
use crate::error::{CliError, Exit};
use crate::output::Output;
use crate::reports::clusters::cluster_build_summary;

impl ClusterBuildArgs {
    /// Builds clusters from stored vectors; incomplete vector coverage exits as partial.
    pub async fn run(
        self,
        path: &Path,
        output: Output,
        verbose: u8,
        config: ForgesyncConfig,
        cancellation: &CancellationToken,
    ) -> Result<Exit, CliError> {
        let request = self.prepare(config.embeddings, config.documents.recipe)?;
        if verbose > 0 && !output.is_json() {
            eprintln!(
                "forgesync: building local clusters for {} using stored vectors",
                request.repository.as_url()
            );
        }
        let report = with_archive(Archive::open_read_write(path), async |archive| {
            build_clusters(archive, &request, cancellation).await
        })
        .await?;
        let exit = if report.generation.complete_coverage {
            Exit::Success
        } else {
            Exit::Partial
        };
        Ok(output.report(&report, cluster_build_summary, exit))
    }

    /// Applies identity overrides and builds the engine request without reading credentials.
    fn prepare(
        self,
        mut service: EmbeddingServiceConfig,
        recipe: DocumentRecipe,
    ) -> Result<ClusterBuildRequest, ConfigError> {
        self.service.apply(&mut service);
        let EmbeddingServiceIdentity { endpoint, model } = service.identity()?;
        Ok(ClusterBuildRequest {
            repository: self.repository,
            endpoint,
            model,
            recipe,
            options: ClusterOptions {
                threshold: self.threshold,
                cross_kind_threshold: self.cross_kind_threshold,
                fanout: self.fanout,
                max_cluster_size: self.max_cluster_size,
                min_cluster_size: self.min_cluster_size,
            },
        })
    }
}

#[cfg(test)]
#[path = "build_tests.rs"]
mod tests;
