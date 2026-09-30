//! # Build and inspect a cluster generation
//!
//! `build_clusters` gathers eligible archived threads, computes candidates, and commits a
//! generation through store methods. Its report separates completed work from failures so the
//! caller can explain the outcome.
//!
//! `list_clusters` reads stored results rather than recomputing similarities. Candidate
//! construction lives below in `candidates`; generation persistence belongs to the store. This
//! separation makes the analysis choice and the durable write visible at different entry points.

use std::sync::{Arc, OnceLock};

use forgesync_core::document::DocumentRecipe;
use forgesync_core::identity::RepositoryId;
use forgesync_store::archive::Archive;
use forgesync_store::clusters::{
    ClusterGenerationInput, ClusterInput, ClusterListQuery as StoreClusterListQuery,
    ClusterMemberInput, ClusterPage,
};
use forgesync_store::embeddings::{EmbeddingDocumentQuery, EmbeddingSearchDocument};
use forgesync_store::leases::ArchiveLeaseToken;
use forgesync_store::reads::{ThreadQuery, ThreadSort, ThreadStateFilter};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use super::candidates::build_cluster_candidates;
use super::lease::ClusterBuildLease;
use super::proposals::ClusterCandidate;
use super::{ClusterBuildReport, ClusterBuildRequest, ClusterListRequest, ClusterOptions};
use crate::documents::now_utc;
use crate::error::EngineError;
use crate::inspect::{checked_page, resolve_repositories};

/// Archive page size used by eligible-thread counting and compatible-vector traversal.
const CLUSTER_PAGE_SIZE: u32 = 500;
/// Process-wide bound on simultaneous CPU-heavy graph builds.
const CLUSTER_WORKER_LIMIT: usize = 1;

/// Shared permits retained by blocking workers until graph construction actually ends.
static CLUSTER_WORKER_SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();

/// Builds and persists deterministic clusters from current open discussions and stored vectors.
///
/// The archive lease fences the vector snapshot and generation write from concurrent archive
/// mutations. Incomplete vector coverage produces a partial run, which cannot retire unseen
/// clusters.
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
    let endpoint = request.endpoint.trim();
    let model = request.model.trim();
    let repositories =
        resolve_repositories(archive, std::slice::from_ref(&request.repository)).await?;
    let repository = repositories
        .first()
        .cloned()
        .ok_or(EngineError::InvalidClusterInput)?;
    let eligible_threads = count_open_threads(archive, &repositories, cancellation).await?;
    let documents = load_cluster_vectors(
        archive,
        &repositories,
        endpoint,
        model,
        request.recipe,
        cancellation,
    )
    .await?;
    let vector_threads = u64::try_from(documents.len())
        .map_err(|_| forgesync_store::error::StoreError::IntegerOutOfRange)?;
    if documents.len() > usize::try_from(eligible_threads).unwrap_or(usize::MAX) {
        return Err(EngineError::InvalidClusterInput);
    }
    if eligible_threads > 0 && documents.is_empty() {
        return Err(EngineError::ClusterVectorsUnavailable);
    }
    let complete_coverage = vector_threads == eligible_threads;
    let repository_full_name = documents
        .first()
        .map(|document| document.summary.repository.full_name.clone())
        .unwrap_or_default();
    let (candidates, candidate_edges) = build_cluster_candidates_bounded(
        documents,
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
    let clusters = candidates
        .into_iter()
        .map(|cluster| ClusterInput {
            representative: cluster.representative,
            title: cluster.title,
            members: cluster
                .members
                .into_iter()
                .map(|member| ClusterMemberInput {
                    thread: member.summary.discussion.id,
                    score_to_representative: member.score_to_representative,
                })
                .collect(),
        })
        .collect();
    let input = ClusterGenerationInput {
        repository,
        endpoint: endpoint.to_owned(),
        model: model.to_owned(),
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

/// Measures eligible open discussions for cluster coverage reporting.
async fn count_open_threads(
    archive: &Archive,
    repositories: &[RepositoryId],
    cancellation: &CancellationToken,
) -> Result<u64, EngineError> {
    let mut offset = 0_u64;
    let mut total = 0_u64;
    loop {
        if cancellation.is_cancelled() {
            return Err(EngineError::ClusteringCancelled);
        }
        let page = archive
            .query_threads(&ThreadQuery {
                repositories: repositories.to_vec(),
                kind: None,
                state: ThreadStateFilter::Open,
                match_expression: None,
                updated_since: None,
                sort: ThreadSort::Created,
                limit: std::num::NonZeroU32::new(CLUSTER_PAGE_SIZE)
                    .expect("cluster page size is non-zero"),
                offset,
            })
            .await?;
        total = total
            .checked_add(
                u64::try_from(page.items.len())
                    .map_err(|_| forgesync_store::error::StoreError::IntegerOutOfRange)?,
            )
            .ok_or(forgesync_store::error::StoreError::IntegerOutOfRange)?;
        let Some(next_offset) = page.next_offset else {
            return Ok(total);
        };
        offset = next_offset;
    }
}

/// Loads current compatible vectors before graph construction.
async fn load_cluster_vectors(
    archive: &Archive,
    repositories: &[RepositoryId],
    endpoint: &str,
    model: &str,
    recipe: DocumentRecipe,
    cancellation: &CancellationToken,
) -> Result<Vec<EmbeddingSearchDocument>, EngineError> {
    let limit =
        std::num::NonZeroU32::new(CLUSTER_PAGE_SIZE).expect("cluster page size is non-zero");
    let mut after_document_id = None;
    let mut documents = Vec::new();
    loop {
        if cancellation.is_cancelled() {
            return Err(EngineError::ClusteringCancelled);
        }
        let page = archive
            .embedding_search_page(&EmbeddingDocumentQuery {
                repositories,
                kind: None,
                state: ThreadStateFilter::Open,
                endpoint,
                model,
                recipe,
                after_document_id,
                limit,
            })
            .await?;
        documents.extend(page.items);
        let Some(next_document_id) = page.next_document_id else {
            return Ok(documents);
        };
        after_document_id = Some(next_document_id);
    }
}

/// Constructs candidate edges under configured similarity and memory bounds.
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
