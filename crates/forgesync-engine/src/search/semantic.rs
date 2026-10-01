//! Score locally stored compatible vectors against one query vector.
//!
//! Document vectors are read from the archive, but the query text is sent to the embedding service
//! only once an eligible archived candidate exists. Reads do not share a transaction, so concurrent
//! materialization can advance between pages; this is current local evidence, not a snapshot.

use std::num::NonZeroU32;

use forgesync_core::document::DocumentRecipe;
use forgesync_core::identity::RepositoryId;
use forgesync_store::archive::Archive;
use forgesync_store::embeddings::{
    EmbeddingDocumentPage, EmbeddingDocumentQuery, EmbeddingSearchDocument,
};
use forgesync_store::reads::FamilyCoverageSummary;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::embedding_client::EmbeddingClient;
use crate::error::EngineError;
use crate::inspect::ThreadSort;
use crate::query::resolve_repositories;
use crate::scoring::{ScoredThread, TopScored};
use crate::search::ranking::{ResultPageRequest, result_page};
use crate::search::{
    SearchHit, SearchMode, SearchProvenance, SearchRanking, SearchRequest, SearchResultPage,
};

/// Number of archived documents read per keyset page.
const EMBEDDING_READ_PAGE: NonZeroU32 = NonZeroU32::new(128).unwrap();

/// Ranks the current compatible documents with exact cosine similarity.
///
/// Pages are streamed to one blocking scorer that keeps at most `candidate_limit` winners.
/// No dimension-compatible document returns [`EngineError::SemanticVectorsUnavailable`]; keyword
/// fallback is the caller's policy.
pub async fn semantic_candidates(
    archive: &Archive,
    request: &SearchRequest,
    recipe: DocumentRecipe,
    client: &EmbeddingClient,
    candidate_limit: usize,
    cancellation: &CancellationToken,
) -> Result<Vec<ScoredThread>, EngineError> {
    if cancellation.is_cancelled() {
        return Err(EngineError::Cancelled);
    }
    let source = SemanticSource {
        archive,
        request,
        recipe,
        client,
        repositories: resolve_repositories(archive, &request.filters.repositories).await?,
        cancellation,
    };
    let first_page = source.first_compatible_page().await?;
    let mut vectors = client
        .embed(&[request.query.trim().to_owned()], cancellation)
        .await?;
    let query = vectors
        .pop()
        .ok_or(EngineError::EmbeddingServiceUnavailable)?;
    let sort = request.filters.sort.unwrap_or(ThreadSort::Relevance);

    let (pages, mut received) = mpsc::channel::<Vec<EmbeddingSearchDocument>>(2);
    let worker_cancellation = cancellation.clone();
    let scorer = tokio::task::spawn_blocking(move || {
        let mut ranking = TopScored::new(query, sort, candidate_limit);
        while let Some(documents) = received.blocking_recv() {
            ranking.add(documents, &worker_cancellation)?;
        }
        Ok::<_, EngineError>(ranking.finish())
    });
    let read = source.stream(first_page, &pages).await;
    drop(pages);
    let scored = scorer.await.map_err(|_| EngineError::SearchWorkerFailed)?;
    read?;
    let (ranked, compatible) = scored?;
    if compatible == 0 {
        return Err(EngineError::SemanticVectorsUnavailable);
    }
    Ok(ranked)
}

/// Candidate scope shared by the availability probe and the full traversal.
struct SemanticSource<'a> {
    archive: &'a Archive,
    request: &'a SearchRequest,
    recipe: DocumentRecipe,
    client: &'a EmbeddingClient,
    repositories: Vec<RepositoryId>,
    cancellation: &'a CancellationToken,
}

impl SemanticSource<'_> {
    /// Reads one raw keyset page under this search's compatibility and filter scope.
    async fn page(&self, cursor: Option<i64>) -> Result<EmbeddingDocumentPage, EngineError> {
        if self.cancellation.is_cancelled() {
            return Err(EngineError::Cancelled);
        }
        let query = EmbeddingDocumentQuery {
            repositories: &self.repositories,
            kind: self.request.filters.kind,
            state: self.request.filters.state,
            endpoint: self.client.endpoint_identity(),
            model: self.client.model(),
            recipe: self.recipe,
            after_document_id: cursor,
            limit: EMBEDDING_READ_PAGE,
        };
        Ok(self.archive.embedding_search_page(&query).await?)
    }

    /// Finds the first page with a document of the configured dimension, so query text is not
    /// sent when no archived candidate could match.
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

    /// Sends `first` and every later page to the scorer; stops early if the scorer has exited.
    async fn stream(
        &self,
        first: EmbeddingDocumentPage,
        pages: &mpsc::Sender<Vec<EmbeddingSearchDocument>>,
    ) -> Result<(), EngineError> {
        let mut page = first;
        loop {
            let next = page.next_document_id;
            if pages.send(page.items).await.is_err() {
                return Ok(());
            }
            let Some(cursor) = next else {
                return Ok(());
            };
            page = self.page(Some(cursor)).await?;
        }
    }
}

/// Projects an ordered semantic prefix into cosine hits and pages the requested window.
///
/// Source ranks are one-based before slicing, so skipped results retain their positions.
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
