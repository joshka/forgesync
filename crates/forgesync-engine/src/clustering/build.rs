//! Build and inspect cluster generations.

use std::time::Duration;

use forgesync_store::archive::Archive;
use forgesync_store::clusters::{
    ClusterGenerationInput, ClusterInput, ClusterListQuery as StoreClusterListQuery,
    ClusterMemberInput, ClusterPage,
};
use forgesync_store::embeddings::EmbeddingSearchDocument;
use forgesync_store::leases::ArchiveLeaseToken;
use tokio_util::sync::CancellationToken;

use super::candidates::build_cluster_candidates;
use super::proposals::ClusterCandidate;
use super::snapshot::ClusterSnapshot;
use super::{ClusterBuildReport, ClusterBuildRequest, ClusterListRequest, ClusterOptions};
use crate::clock::now_utc;
use crate::error::EngineError;
use crate::lease::with_writer_lease;
use crate::query::{checked_page, resolve_repositories};

/// Writer lease lifetime for generation builds and maintainer decisions.
pub const CLUSTER_LEASE_DURATION: Duration = Duration::from_secs(180);

/// Builds and persists deterministic clusters from current open discussions and stored vectors.
///
/// The archive lease fences the vector snapshot and generation write from concurrent archive
/// mutations. Incomplete vector coverage produces a partial run, which cannot retire unseen
/// clusters. This operation reads stored vectors and does not contact GitHub or a model service.
/// Endpoint, model, and recipe must identify the vectors already materialized in the archive.
///
/// # Persistence and cancellation
///
/// A writer lease spans evidence loading and generation application. The store commits the
/// generation transaction; earlier source observations and vectors are not rewritten. Cancellation
/// is checked during paging and candidate analysis, then again before saving. Interruption or lease
/// renewal failure signals a child token and waits for the build before releasing the fence.
///
/// # Errors
///
/// Invalid graph options or service identity fail before acquiring the lease. A nonempty open
/// discussion scope without compatible vectors returns [`EngineError::ClusterVectorsUnavailable`].
/// Inconsistent coverage, archive reads/writes, worker failure, and cancellation retain typed
/// errors. Partial vector coverage is a successful report with `complete_coverage` false, rather
/// than an error; such a generation cannot retire unseen clusters.
pub async fn build_clusters(
    archive: &Archive,
    request: &ClusterBuildRequest,
    cancellation: &CancellationToken,
) -> Result<ClusterBuildReport, EngineError> {
    request.options.validate()?;
    if request.endpoint.trim().is_empty()
        || request.model.trim().is_empty()
        || request.endpoint.len() > 2048
        || request.model.len() > 512
    {
        return Err(EngineError::InvalidClusterInput);
    }
    if cancellation.is_cancelled() {
        return Err(EngineError::ClusteringCancelled);
    }

    with_writer_lease(
        archive,
        CLUSTER_LEASE_DURATION,
        cancellation,
        async |lease, cancellation| {
            execute_cluster_build(archive, request, lease, cancellation).await
        },
    )
    .await
}

/// Reads one page of persisted clusters without contacting GitHub or mutating the archive.
pub async fn list_clusters(
    archive: &Archive,
    request: &ClusterListRequest,
) -> Result<ClusterPage, EngineError> {
    let (limit, offset) = checked_page(request.limit, request.offset)?;
    let repositories = resolve_repositories(archive, &request.repositories).await?;
    let query = StoreClusterListQuery {
        repositories: &repositories,
        include_retired: request.include_retired,
        limit,
        offset,
    };
    Ok(archive.list_clusters(&query).await?)
}

/// Loads compatible evidence and persists one deterministic cluster generation.
async fn execute_cluster_build(
    archive: &Archive,
    request: &ClusterBuildRequest,
    lease: &ArchiveLeaseToken,
    cancellation: &CancellationToken,
) -> Result<ClusterBuildReport, EngineError> {
    let snapshot = ClusterSnapshot::load(archive, request, cancellation).await?;
    let repository_full_name = snapshot.repository_full_name();
    let eligible_threads = snapshot.eligible_threads;
    let vector_threads = snapshot.vector_threads;
    let complete_coverage = snapshot.complete_coverage();
    let (candidates, candidate_edges) = build_cluster_candidates_bounded(
        snapshot.documents,
        repository_full_name,
        request.options,
        cancellation,
    )
    .await?;
    if cancellation.is_cancelled() {
        return Err(EngineError::ClusteringCancelled);
    }
    let candidate_edges = u64::try_from(candidate_edges)
        .map_err(|_| forgesync_store::error::StoreError::IntegerOutOfRange)?;
    let clusters = candidates.into_iter().map(cluster_input).collect();
    let input = ClusterGenerationInput {
        repository: snapshot.repository,
        endpoint: request.endpoint.trim().to_owned(),
        model: request.model.trim().to_owned(),
        recipe: request.recipe,
        complete_coverage,
        eligible_threads,
        vector_threads,
        candidate_edges,
        clusters,
    };
    let generation = archive
        .save_clusters_fenced(lease, &input, now_utc()?)
        .await?;
    Ok(ClusterBuildReport {
        generation,
        eligible_threads,
        vector_threads,
        candidate_edges,
    })
}

/// Converts one analyzed proposal into the store's generation input without changing member order.
fn cluster_input(cluster: ClusterCandidate) -> ClusterInput {
    let members = cluster
        .members
        .into_iter()
        .map(|member| ClusterMemberInput {
            thread: member.summary.discussion.id,
            score_to_representative: member.score_to_representative,
        })
        .collect();
    ClusterInput {
        representative: cluster.representative,
        title: cluster.title,
        members,
    }
}

/// Runs CPU-heavy graph construction on a blocking worker; analysis checks the cloned token.
async fn build_cluster_candidates_bounded(
    documents: Vec<EmbeddingSearchDocument>,
    repository_full_name: String,
    options: ClusterOptions,
    cancellation: &CancellationToken,
) -> Result<(Vec<ClusterCandidate>, usize), EngineError> {
    let cancellation = cancellation.clone();
    tokio::task::spawn_blocking(move || {
        build_cluster_candidates(documents, &repository_full_name, options, &cancellation)
    })
    .await
    .map_err(|_| EngineError::ClusterWorkerFailed)?
}
