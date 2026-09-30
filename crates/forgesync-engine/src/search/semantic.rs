//! # Score locally stored compatible vectors
//!
//! Semantic helpers obtain candidate documents and chunks, reject incompatible dimensions, and
//! score a bounded page with exact similarity. They return scored threads to the shared ranking
//! path.
//!
//! Document vectors are read from the archive, but the query text is sent to the configured
//! embedding service once compatible archived candidates are found. Keyword search is the offline
//! alternative. Compatibility checks reject vectors left behind by model or recipe changes.
//!
//! `SemanticSource` retains one repository/filter/endpoint/model/recipe scope for the availability
//! probe and subsequent keyset reads. The first eligible page is reused for scoring rather than
//! fetched twice. Reads do not share a transaction, so concurrent materialization can advance
//! between pages; this is current local evidence, not a historical snapshot.
//!
//! `SemanticRanking` holds the generated query vector, requested ordering, bounded winning prefix,
//! and compatible-document count. Exact scoring and deterministic tie ordering belong to
//! `exact_search`. The separate compatibility count prevents a zero-sized result bound from being
//! mistaken for unavailable vectors. Archive decoding owns vector validity and nonempty chunks.
//!
//! Blocking scoring is capped by process-wide permits, including concurrent search requests. A
//! worker retains its permit until it returns; cancellation is checked while waiting and by the
//! scorer between candidates. No vectors, documents, or coverage are written during retrieval.
//! [`semantic_result_page`] projects the already ordered prefix into visible cosine provenance
//! before the shared pagination operation; cosine similarity is evidence, not a probability.

use std::num::NonZeroU32;
use std::sync::{Arc, OnceLock};

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

use crate::embedding_client::EmbeddingClient;
use crate::error::EngineError;
use crate::inspect::ThreadSort;
use crate::query::{resolve_repositories, store_sort, store_state_filter};
use crate::scoring::{ScoredThread, merge_scored_pages, score_embedding_page};
use crate::search::ranking::{ResultPageRequest, result_page};
use crate::search::{
    SearchHit, SearchMode, SearchProvenance, SearchRanking, SearchRequest, SearchResultPage,
};

/// Number of archived documents read per exact-scoring batch.
const EMBEDDING_READ_PAGE: u32 = 128;
/// Process-wide cap on concurrently executing exact-scoring workers.
const EXACT_WORKER_LIMIT: usize = 2;
/// Shared permits bound blocking scoring across concurrent search requests.
static EXACT_SEARCH_SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();

/// Acquires and ranks the current compatible document prefix with exact cosine similarity.
///
/// Resolves repository selectors before reading candidates and uses one source scope throughout.
/// Query text is trimmed and sent to the embedding service only after an eligible archived page
/// exists. At most `candidate_limit` winners are retained while every eligible archive page is
/// considered. Per-document scoring and stable ties are delegated to `exact_search`.
///
/// # Errors
///
/// Returns cancellation, repository resolution, archive, embedding, or worker failures with their
/// typed classification. No eligible dimension-compatible collection returns
/// [`EngineError::SemanticVectorsUnavailable`]. This operation does not select keyword fallback;
/// its caller applies that policy.
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
    /// Already opened archive used for current vector pages.
    archive: &'a Archive,
    /// Original repository, kind, state, query, and sort interpretation.
    request: &'a SearchRequest,
    /// Document rendering recipe required of stored vectors.
    recipe: DocumentRecipe,
    /// Endpoint/model identity, dimension expectation, and query-vector service.
    client: &'a EmbeddingClient,
    /// Resolved durable repository identities shared by every page.
    repositories: Vec<RepositoryId>,
    /// Nonzero read-batch size, independent of the result bound.
    limit: NonZeroU32,
    /// Workflow cancellation shared by reads and query embedding.
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
    /// Generated query vector used by every scoring batch.
    query: EmbeddingVector,
    /// Requested deterministic ordering applied during bounded merges.
    sort: ThreadSort,
    /// Maximum winning prefix retained, including skipped presentation positions.
    limit: usize,
    /// Current winning prefix in final ordering.
    ranked: Vec<ScoredThread>,
    /// Compatibility evidence independent of the number of retained winners.
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

/// Counts documents whose every decoded chunk has the query dimension.
///
/// Endpoint, model, recipe, and source-currentness are filtered by the archive query before this
/// helper runs; this predicate checks dimensions only. Archive decoding requires nonempty chunks.
/// An independently constructed empty chunk vector satisfies `all` and counts here, so this helper
/// must not be mistaken for standalone document validation.
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

/// Scores an archive page on a blocking worker while holding a process-wide permit.
///
/// Cancellation while waiting returns immediately. Once started, the scorer observes cancellation
/// between candidates; this method awaits the worker result, and the permit remains held until the
/// worker exits even if the awaiting future is dropped. A join failure returns
/// [`EngineError::SearchWorkerFailed`]; scoring errors retain their classification.
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

/// Projects an ordered semantic prefix into cosine hits and pages the requested window.
///
/// Reads query, mode, sort, and validated coordinates from the original request. Source ranks are
/// one-based before slicing, so skipped results retain their positions. Scores and coverage are
/// preserved without revalidation or freshness mutation; the caller has already acquired and ranked
/// compatible evidence. The effective mode is semantic and no fallback reason is attached.
pub fn semantic_result_page(
    request: &SearchRequest,
    candidates: Vec<ScoredThread>,
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
        query: request.query.trim(),
        requested_mode: request.mode,
        mode: SearchMode::Semantic,
        ranking: SearchRanking::Cosine,
        sort: request.filters.sort.unwrap_or(ThreadSort::Relevance),
        fallback_reason: None,
        candidates: items,
        offset: request.filters.offset,
        limit: request.filters.limit,
        coverage,
    })
}
