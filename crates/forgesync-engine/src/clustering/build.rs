//! # Build and inspect a cluster generation
//!
//! `build_clusters` gathers eligible archived threads, computes candidates, and commits a
//! generation through store methods. Its report separates completed work from failures so the
//! caller can explain the outcome.
//!
//! `list_clusters` reads stored results rather than recomputing similarities. Candidate
//! construction lives below in `candidates`; `snapshot` resolves current vector evidence and
//! independent source/vector coverage counts. Generation persistence belongs to the store.
//! The coordinator keeps snapshot loading, bounded graph work, and generation application in
//! reading order, while `lease` retains renewal and cooperative cleanup across those phases.

use std::sync::{Arc, OnceLock};

use forgesync_store::archive::Archive;
use forgesync_store::clusters::{
    ClusterGenerationInput, ClusterInput, ClusterListQuery as StoreClusterListQuery,
    ClusterMemberInput, ClusterPage,
};
use forgesync_store::embeddings::EmbeddingSearchDocument;
use forgesync_store::leases::ArchiveLeaseToken;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use super::candidates::build_cluster_candidates;
use super::lease::ClusterBuildLease;
use super::proposals::ClusterCandidate;
use super::snapshot::ClusterSnapshot;
use super::{ClusterBuildReport, ClusterBuildRequest, ClusterListRequest, ClusterOptions};
use crate::clock::now_utc;
use crate::error::EngineError;
use crate::query::{checked_page, resolve_repositories};

/// Process-wide bound on simultaneous CPU-heavy graph builds.
const CLUSTER_WORKER_LIMIT: usize = 1;

/// Shared permits retained by blocking workers until graph construction actually ends.
static CLUSTER_WORKER_SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();

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

    let lease = ClusterBuildLease::acquire(archive, cancellation).await?;
    let operation = execute_cluster_build(archive, request, &lease.token, &lease.cancellation);
    lease.complete(operation, cancellation).await
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

/// Constructs candidate edges under configured similarity and memory bounds.
///
/// The process-wide permit bounds active graph workers, including builds in other archives.
/// Cancellation while waiting returns without starting a worker. Once started, graph construction
/// checks the cloned token and this adapter awaits its result. The worker retains its permit until
/// it exits even if the awaiting future is dropped; the lease coordinator separately ensures normal
/// cancellation drains the operation before releasing writer authority.
///
/// Documents, reference context, and options move together into the worker; no archive connection
/// crosses that boundary. A join failure becomes `ClusterWorkerFailed`, while analysis errors
/// retain their classification. The returned edge count precedes component-size pruning.
async fn build_cluster_candidates_bounded(
    documents: Vec<EmbeddingSearchDocument>,
    repository_full_name: String,
    options: ClusterOptions,
    cancellation: &CancellationToken,
) -> Result<(Vec<ClusterCandidate>, usize), EngineError> {
    let slots = Arc::clone(
        CLUSTER_WORKER_SLOTS.get_or_init(|| Arc::new(Semaphore::new(CLUSTER_WORKER_LIMIT))),
    );
    let permit = tokio::select! {
        _ = cancellation.cancelled() => return Err(EngineError::ClusteringCancelled),
        permit = slots.acquire_owned() => permit.map_err(|_| EngineError::ClusterWorkerFailed)?,
    };
    let worker_cancellation = cancellation.clone();
    tokio::task::spawn_blocking(move || {
        let _permit: OwnedSemaphorePermit = permit;
        build_cluster_candidates(
            documents,
            &repository_full_name,
            options,
            &worker_cancellation,
        )
    })
    .await
    .map_err(|_| EngineError::ClusterWorkerFailed)?
}
