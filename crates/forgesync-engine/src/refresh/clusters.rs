//! # Add cluster analysis to a refresh
//!
//! `build_repository_clusters` visits the selected repository scope after acquisition, retaining
//! one outcome for each attempted repository. `ClusterStage` keeps service identity, recipe, and
//! graph policy consistent across requests and owns aggregate coverage/failure accounting.
//!
//! Missing service identity becomes a per-repository failure without attempting a generation.
//! Other repository failures are isolated so later repositories can still succeed. Cancellation
//! stops traversal; previously completed generations remain durable and represented in the report.
//!
//! The first failure remains the primary diagnostic. That diagnostic's cancellation code determines
//! interruption; otherwise mixed success/failure or incomplete vector coverage produces a partial
//! stage. This preserves the existing report policy rather than deriving status from the last
//! result. Candidate analysis and fencing remain in `clustering`, and this adapter never rewrites
//! source observations. Nearby cases isolate stage accounting from archive and provider setup.

use std::ops::ControlFlow;

use forgesync_core::document::DocumentRecipe;
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;

use crate::clustering::{ClusterBuildReport, ClusterBuildRequest, ClusterOptions, build_clusters};
use crate::reference::RepositorySelector;
use crate::refresh::status::{keep_first_failure, stage_failure};
use crate::refresh::{
    EmbeddingServiceIdentity, RefreshClusterRepository, RefreshStage, RefreshStageFailure,
    RefreshStageStatus,
};

/// Generates independently reported cluster generations using one identity and policy.
/// Cancellation leaves attempted repository outcomes intact and stops remaining work.
pub async fn build_repository_clusters(
    archive: &Archive,
    repositories: &[RepositorySelector],
    identity: Option<&EmbeddingServiceIdentity>,
    recipe: DocumentRecipe,
    options: ClusterOptions,
    cancellation: &CancellationToken,
) -> RefreshStage<Vec<RefreshClusterRepository>> {
    let mut stage = ClusterStage {
        identity,
        recipe,
        options,
        results: Vec::with_capacity(repositories.len()),
        first_failure: None,
        has_partial_coverage: false,
    };
    for repository in repositories {
        if cancellation.is_cancelled() {
            stage.interrupt();
            break;
        }
        if stage
            .build(archive, repository, cancellation)
            .await
            .is_break()
        {
            break;
        }
    }
    stage.finish()
}

/// Request policy and accumulated repository outcomes for one selected refresh stage.
struct ClusterStage<'a> {
    /// Identity of the stored vector service, absent when setup could not establish it.
    identity: Option<&'a EmbeddingServiceIdentity>,
    /// Document recipe shared by every repository build.
    recipe: DocumentRecipe,
    /// Graph policy shared by every repository build.
    options: ClusterOptions,
    /// Outcomes of attempted repositories in traversal order, excluding untouched cancelled work.
    results: Vec<RefreshClusterRepository>,
    /// First primary diagnostic, retained even when later repositories fail differently.
    first_failure: Option<RefreshStageFailure>,
    /// Whether any successful generation had incomplete compatible-vector coverage.
    has_partial_coverage: bool,
}

impl ClusterStage<'_> {
    /// Attempts one repository and records its independent outcome before deciding to continue.
    async fn build(
        &mut self,
        archive: &Archive,
        repository: &RepositorySelector,
        cancellation: &CancellationToken,
    ) -> ControlFlow<()> {
        let result = self.attempt(archive, repository, cancellation).await;
        let failed = result.is_err();
        match result {
            Ok(report) => self.record_success(repository, report),
            Err(failure) => self.record_failure(repository, failure),
        }
        if failed && cancellation.is_cancelled() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    /// Converts the shared request policy into a repository build or a missing-identity failure.
    async fn attempt(
        &self,
        archive: &Archive,
        repository: &RepositorySelector,
        cancellation: &CancellationToken,
    ) -> Result<ClusterBuildReport, RefreshStageFailure> {
        let identity = self.identity.ok_or_else(missing_identity)?;
        let request = ClusterBuildRequest {
            repository: repository.clone(),
            endpoint: identity.endpoint.clone(),
            model: identity.model.clone(),
            recipe: self.recipe,
            options: self.options,
        };
        build_clusters(archive, &request, cancellation)
            .await
            .map_err(|error| stage_failure(&error))
    }

    /// Retains a successful generation and its coverage contribution to aggregate status.
    fn record_success(&mut self, repository: &RepositorySelector, report: ClusterBuildReport) {
        self.has_partial_coverage |= !report.generation.complete_coverage;
        self.results.push(RefreshClusterRepository {
            repository: repository.as_url(),
            report: Some(report),
            failure: None,
        });
    }

    /// Records a repository failure without discarding previous successes or replacing the first
    /// diagnostic.
    fn record_failure(&mut self, repository: &RepositorySelector, failure: RefreshStageFailure) {
        keep_first_failure(&mut self.first_failure, failure.clone());
        self.results.push(RefreshClusterRepository {
            repository: repository.as_url(),
            report: None,
            failure: Some(failure),
        });
    }

    /// Records caller interruption before the next repository without inventing an attempted
    /// result.
    fn interrupt(&mut self) {
        keep_first_failure(
            &mut self.first_failure,
            RefreshStageFailure {
                code: "operation_cancelled",
                message: "clustering was cancelled with repositories remaining".to_owned(),
            },
        );
    }

    /// Projects accumulated outcomes into the public stage representation.
    fn finish(self) -> RefreshStage<Vec<RefreshClusterRepository>> {
        let status = self.status();
        RefreshStage::with_report(status, self.results, self.first_failure)
    }

    /// Classifies the retained primary diagnostic and aggregate coverage, including an empty scope.
    fn status(&self) -> RefreshStageStatus {
        if self
            .first_failure
            .as_ref()
            .is_some_and(|failure| failure.code == "operation_cancelled")
        {
            RefreshStageStatus::Interrupted
        } else if self.first_failure.is_some() {
            if self.results.iter().any(|result| result.report.is_some()) {
                RefreshStageStatus::Partial
            } else {
                RefreshStageStatus::Failed
            }
        } else if self.has_partial_coverage {
            RefreshStageStatus::Partial
        } else {
            RefreshStageStatus::Complete
        }
    }
}

/// Reports absent compatible service identity without attempting archive analysis.
fn missing_identity() -> RefreshStageFailure {
    RefreshStageFailure {
        code: "embedding_service_identity_missing",
        message: "clustering requires an endpoint and model matching stored vectors".to_owned(),
    }
}

#[cfg(test)]
#[path = "clusters_tests.rs"]
mod tests;
