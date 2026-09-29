//! # Score locally stored compatible vectors
//!
//! Semantic helpers obtain candidate documents and chunks, reject incompatible dimensions, and
//! score a bounded page with exact similarity. They return scored threads to the shared ranking
//! path.
//!
//! Document vectors are read from the archive, but the query text is sent to the configured
//! embedding service once compatible archived candidates are found. Keyword search is the offline
//! alternative. Compatibility checks reject vectors left behind by model or recipe changes.

use std::num::NonZeroU32;
use std::sync::Arc;

use forgesync_core::document::DocumentRecipe;
use forgesync_core::embedding::EmbeddingVector;
use forgesync_core::identity::RepositoryId;
use forgesync_store::archive::Archive;
use forgesync_store::embeddings::{
    EmbeddingDocumentPage, EmbeddingDocumentQuery, EmbeddingSearchDocument,
};
use forgesync_store::reads::FamilyCoverageSummary;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use super::ranking::result_page;
use super::{
    EMBEDDING_READ_PAGE, EXACT_SEARCH_SLOTS, EXACT_WORKER_LIMIT, ResultPageRequest, SearchHit,
    SearchMode, SearchProvenance, SearchRanking, SearchRequest, SearchResultPage,
};
use crate::embedding_client::EmbeddingClient;
use crate::error::EngineError;
use crate::exact_search::{ScoredThread, merge_scored_pages, score_embedding_page};
use crate::inspect::{ThreadSort, resolve_repositories, store_sort, store_state_filter};

/// Ranks current dimension-compatible chunks with exact cosine similarity.
pub async fn semantic_candidates(
    archive: &Archive,
    request: &SearchRequest,
    recipe: DocumentRecipe,
    client: &EmbeddingClient,
    candidate_limit: usize,
    cancellation: &CancellationToken,
) -> Result<Vec<ScoredThread>, EngineError> {
    if cancellation.is_cancelled() {
        return Err(EngineError::SearchCancelled);
    }
    let repositories = resolve_repositories(archive, &request.filters.repositories).await?;
    let limit = NonZeroU32::new(EMBEDDING_READ_PAGE).expect("non-zero page size");
    let source = SemanticSource {
        archive,
        request,
        recipe,
        client,
        repositories,
        limit,
        cancellation,
    };
    let first_page = source.first_compatible_page().await?;
    let query = source.query_vector().await?;
    let mut ranking = SemanticRanking {
        query,
        sort: request.filters.sort.unwrap_or(ThreadSort::Relevance),
        limit: candidate_limit,
        ranked: Vec::with_capacity(candidate_limit),
        compatible_documents: 0,
    };
    let mut next = first_page.next_document_id;
    ranking.add(first_page.items, cancellation).await?;
    while let Some(cursor) = next {
        let page = source.page(Some(cursor)).await?;
        next = page.next_document_id;
        ranking.add(page.items, cancellation).await?;
    }
    ranking.finish()
}

/// Immutable candidate scope shared by the availability probe and full scoring traversal.
struct SemanticSource<'a> {
    archive: &'a Archive,
    request: &'a SearchRequest,
    recipe: DocumentRecipe,
    client: &'a EmbeddingClient,
    repositories: Vec<RepositoryId>,
    limit: NonZeroU32,
    cancellation: &'a CancellationToken,
}

impl SemanticSource<'_> {
    /// Reads a raw keyset page using the same compatibility and filter scope throughout a search.
    async fn page(&self, cursor: Option<i64>) -> Result<EmbeddingDocumentPage, EngineError> {
        if self.cancellation.is_cancelled() {
            return Err(EngineError::SearchCancelled);
        }
        let query = EmbeddingDocumentQuery {
            repositories: &self.repositories,
            kind: self.request.filters.kind,
            state: store_state_filter(self.request.filters.state),
            endpoint: self.client.endpoint_identity(),
            model: self.client.model(),
            recipe: self.recipe,
            after_document_id: cursor,
            limit: self.limit,
        };
        Ok(self.archive.embedding_search_page(&query).await?)
    }

    /// Avoids sending query text to the service when no eligible archived document exists.
    async fn first_compatible_page(&self) -> Result<EmbeddingDocumentPage, EngineError> {
        let mut cursor = None;
        loop {
            let page = self.page(cursor).await?;
            let compatible = page.items.iter().any(|document| {
                document.chunks.first().is_some_and(|chunk| {
                    self.client
                        .dimensions()
                        .is_none_or(|size| chunk.vector.dimensions() == size)
                })
            });
            if compatible {
                return Ok(page);
            }
            cursor = Some(
                page.next_document_id
                    .ok_or(EngineError::SemanticVectorsUnavailable)?,
            );
        }
    }

    /// Generates exactly one query vector after archived candidate availability is established.
    async fn query_vector(&self) -> Result<EmbeddingVector, EngineError> {
        let text = self.request.query.trim().to_owned();
        let mut vectors = self.client.embed(&[text], self.cancellation).await?;
        vectors
            .pop()
            .ok_or(EngineError::EmbeddingServiceUnavailable)
    }
}

/// Bounded accumulated ranking and compatibility evidence for one generated query vector.
struct SemanticRanking {
    query: EmbeddingVector,
    sort: ThreadSort,
    limit: usize,
    ranked: Vec<ScoredThread>,
    compatible_documents: usize,
}

impl SemanticRanking {
    /// Scores one page and merges it under the global result bound without losing availability.
    async fn add(
        &mut self,
        documents: Vec<EmbeddingSearchDocument>,
        cancellation: &CancellationToken,
    ) -> Result<(), EngineError> {
        self.compatible_documents =
            self.compatible_documents
                .saturating_add(count_dimension_compatible(
                    &documents,
                    self.query.dimensions(),
                ));
        let scores = score_page_bounded(
            self.query.clone(),
            documents,
            self.sort,
            self.limit,
            cancellation,
        )
        .await?;
        merge_scored_pages(&mut self.ranked, scores, store_sort(self.sort), self.limit);
        Ok(())
    }

    /// Distinguishes an empty ranking from an unavailable compatible vector collection.
    fn finish(self) -> Result<Vec<ScoredThread>, EngineError> {
        if self.compatible_documents == 0 {
            return Err(EngineError::SemanticVectorsUnavailable);
        }
        Ok(self.ranked)
    }
}

/// Counts vectors matching the selected model and query dimension.
pub fn count_dimension_compatible(documents: &[EmbeddingSearchDocument], dimensions: u32) -> usize {
    documents
        .iter()
        .filter(|document| {
            document
                .chunks
                .iter()
                .all(|chunk| chunk.vector.dimensions() == dimensions)
        })
        .count()
}

/// Scores one archive page within the exact-search worker limit.
pub async fn score_page_bounded(
    query: EmbeddingVector,
    documents: Vec<EmbeddingSearchDocument>,
    sort: ThreadSort,
    limit: usize,
    cancellation: &CancellationToken,
) -> Result<Vec<ScoredThread>, EngineError> {
    let slots =
        Arc::clone(EXACT_SEARCH_SLOTS.get_or_init(|| Arc::new(Semaphore::new(EXACT_WORKER_LIMIT))));
    let permit = tokio::select! {
        _ = cancellation.cancelled() => return Err(EngineError::SearchCancelled),
        permit = slots.acquire_owned() => permit.map_err(|_| EngineError::SearchWorkerFailed)?,
    };
    let worker_cancellation = cancellation.clone();
    tokio::task::spawn_blocking(move || {
        let _permit: OwnedSemaphorePermit = permit;
        score_embedding_page(
            &query,
            documents,
            store_sort(sort),
            limit,
            &worker_cancellation,
        )
    })
    .await
    .map_err(|_| EngineError::SearchWorkerFailed)?
}

/// Paginates ranked semantic hits with coverage provenance.
pub fn semantic_result_page(
    query: &str,
    requested_mode: SearchMode,
    sort: ThreadSort,
    candidates: Vec<ScoredThread>,
    offset: u64,
    limit: u32,
    coverage: Vec<FamilyCoverageSummary>,
) -> SearchResultPage {
    let items = candidates
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| SearchHit {
            summary: candidate.summary,
            score: Some(candidate.score),
            provenance: vec![SearchProvenance::Semantic {
                rank: u32::try_from(index + 1).unwrap_or(u32::MAX),
                cosine_score: candidate.score,
            }],
        })
        .collect();
    result_page(ResultPageRequest {
        query,
        requested_mode,
        mode: SearchMode::Semantic,
        ranking: SearchRanking::Cosine,
        sort,
        fallback_reason: None,
        candidates: items,
        offset,
        limit,
        coverage,
    })
}
