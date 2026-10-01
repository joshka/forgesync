//! Current archived vector evidence for one generation.
//!
//! Eligible-thread and compatible-vector counts stay distinct: incomplete coverage can produce
//! useful clusters but cannot authorize retirement of unseen groups. No compatible vectors for a
//! nonempty eligible scope is an error; more vector documents than eligible threads is rejected as
//! inconsistent evidence.

use std::num::NonZeroU32;

use forgesync_core::identity::RepositoryId;
use forgesync_store::archive::Archive;
use forgesync_store::embeddings::{EmbeddingDocumentQuery, EmbeddingSearchDocument};
use forgesync_store::reads::{ThreadQuery, ThreadSort, ThreadStateFilter};
use tokio_util::sync::CancellationToken;

use crate::clustering::ClusterBuildRequest;
use crate::error::EngineError;
use crate::query::resolve_repositories;

/// Shared page budget for counting eligible threads and loading compatible vectors.
const CLUSTER_PAGE: NonZeroU32 = NonZeroU32::new(500).unwrap();

/// Repository-scoped source evidence and independent coverage counts before graph analysis.
pub struct ClusterSnapshot {
    /// Durable repository identity resolved from the requested selector.
    pub repository: RepositoryId,
    /// Current open discussions eligible for this generation, including those missing vectors.
    pub eligible_threads: u64,
    /// Compatible vector documents available for candidate analysis.
    pub vector_threads: u64,
    /// Ordered current document/vector evidence, consumed by the blocking graph worker.
    pub documents: Vec<EmbeddingSearchDocument>,
}

impl ClusterSnapshot {
    /// Loads one analysis scope and rejects unavailable or inconsistent vector evidence.
    /// The caller retains its writer fence across these reads and later generation persistence.
    pub async fn load(
        archive: &Archive,
        request: &ClusterBuildRequest,
        cancellation: &CancellationToken,
    ) -> Result<Self, EngineError> {
        let repositories =
            resolve_repositories(archive, std::slice::from_ref(&request.repository)).await?;
        let repository = repositories
            .first()
            .cloned()
            .ok_or(EngineError::InvalidClusterInput)?;
        let eligible_threads = count_open_threads(archive, &repositories, cancellation).await?;
        let documents = load_vectors(archive, &repositories, request, cancellation).await?;
        let vector_threads = documents.len() as u64;
        if vector_threads > eligible_threads {
            return Err(EngineError::InvalidClusterInput);
        }
        if eligible_threads > 0 && documents.is_empty() {
            return Err(EngineError::ClusterVectorsUnavailable);
        }
        Ok(Self {
            repository,
            eligible_threads,
            vector_threads,
            documents,
        })
    }

    /// Reports whether every eligible discussion supplied a current compatible vector document.
    pub fn complete_coverage(&self) -> bool {
        self.vector_threads == self.eligible_threads
    }

    /// Supplies repository-qualified reference context, or an empty name for an empty graph.
    pub fn repository_full_name(&self) -> String {
        self.documents
            .first()
            .map(|document| document.summary.repository.full_name.clone())
            .unwrap_or_default()
    }
}

/// Counts eligible open discussions without treating missing vectors as absent source threads.
async fn count_open_threads(
    archive: &Archive,
    repositories: &[RepositoryId],
    cancellation: &CancellationToken,
) -> Result<u64, EngineError> {
    let mut offset = 0;
    let mut total = 0_u64;
    loop {
        if cancellation.is_cancelled() {
            return Err(EngineError::Cancelled);
        }
        let query = ThreadQuery {
            repositories: repositories.to_vec(),
            kind: None,
            state: ThreadStateFilter::Open,
            match_expression: None,
            updated_since: None,
            sort: ThreadSort::Created,
            limit: CLUSTER_PAGE,
            offset,
        };
        let page = archive.query_threads(&query).await?;
        total += page.items.len() as u64;
        let Some(next_offset) = page.next_offset else {
            return Ok(total);
        };
        offset = next_offset;
    }
}

/// Loads compatible evidence using the raw store cursor, keeping service and source scope fixed.
async fn load_vectors(
    archive: &Archive,
    repositories: &[RepositoryId],
    request: &ClusterBuildRequest,
    cancellation: &CancellationToken,
) -> Result<Vec<EmbeddingSearchDocument>, EngineError> {
    let mut after_document_id = None;
    let mut documents = Vec::new();
    loop {
        if cancellation.is_cancelled() {
            return Err(EngineError::Cancelled);
        }
        let query = EmbeddingDocumentQuery {
            repositories,
            kind: None,
            state: ThreadStateFilter::Open,
            endpoint: request.endpoint.trim(),
            model: request.model.trim(),
            recipe: request.recipe,
            after_document_id,
            limit: CLUSTER_PAGE,
        };
        let page = archive.embedding_search_page(&query).await?;
        documents.extend(page.items);
        let Some(next_document_id) = page.next_document_id else {
            return Ok(documents);
        };
        after_document_id = Some(next_document_id);
    }
}
