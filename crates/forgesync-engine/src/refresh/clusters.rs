//! Refresh clusters behavior.

use super::status::{keep_first_failure, stage_failure};
use super::{
    Archive, CancellationToken, ClusterBuildRequest, ClusterOptions, DocumentRecipe,
    EmbeddingServiceIdentity, RefreshClusterRepository, RefreshStage, RefreshStageFailure,
    RefreshStageStatus, RepositorySelector, build_clusters,
};

pub async fn build_repository_clusters(
    archive: &Archive,
    repositories: &[RepositorySelector],
    identity: Option<&EmbeddingServiceIdentity>,
    recipe: DocumentRecipe,
    options: ClusterOptions,
    cancellation: &CancellationToken,
) -> RefreshStage<Vec<RefreshClusterRepository>> {
    let mut results = Vec::with_capacity(repositories.len());
    let mut first_failure = None;
    let mut has_partial_coverage = false;

    for repository in repositories {
        if cancellation.is_cancelled() {
            keep_first_failure(
                &mut first_failure,
                RefreshStageFailure {
                    code: "operation_cancelled",
                    message: "clustering was cancelled with repositories remaining".to_owned(),
                },
            );
            break;
        }

        let Some(identity) = identity else {
            let failure = RefreshStageFailure {
                code: "embedding_service_identity_missing",
                message: "clustering requires an endpoint and model matching stored vectors"
                    .to_owned(),
            };
            keep_first_failure(&mut first_failure, failure.clone());
            results.push(RefreshClusterRepository {
                repository: repository.as_url(),
                report: None,
                failure: Some(failure),
            });
            continue;
        };

        let request = ClusterBuildRequest {
            repository: repository.clone(),
            endpoint: identity.endpoint.clone(),
            model: identity.model.clone(),
            recipe,
            options,
        };
        match build_clusters(archive, &request, cancellation).await {
            Ok(report) => {
                has_partial_coverage |= !report.generation.complete_coverage;
                results.push(RefreshClusterRepository {
                    repository: repository.as_url(),
                    report: Some(report),
                    failure: None,
                });
            }
            Err(error) => {
                let failure = stage_failure(&error);
                keep_first_failure(&mut first_failure, failure.clone());
                results.push(RefreshClusterRepository {
                    repository: repository.as_url(),
                    report: None,
                    failure: Some(failure),
                });
                if cancellation.is_cancelled() {
                    break;
                }
            }
        }
    }

    let status = if first_failure
        .as_ref()
        .is_some_and(|failure| failure.code == "operation_cancelled")
    {
        RefreshStageStatus::Interrupted
    } else if first_failure.is_some() {
        if results.iter().any(|result| result.report.is_some()) {
            RefreshStageStatus::Partial
        } else {
            RefreshStageStatus::Failed
        }
    } else if has_partial_coverage {
        RefreshStageStatus::Partial
    } else {
        RefreshStageStatus::Complete
    };
    RefreshStage::with_report(status, results, first_failure)
}
