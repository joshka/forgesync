//! # Score locally stored compatible vectors
//!
//! Semantic helpers obtain candidate documents and chunks, reject incompatible dimensions, and
//! score a bounded page with exact similarity. They return scored threads to the shared ranking
//! path.
//!
//! This is an offline read of previously generated embeddings. It never calls the embedding
//! service. Compatibility checks are essential because model or recipe changes can leave older
//! vectors in the archive.

use super::ranking::result_page;
use super::{
    Arc, Archive, CancellationToken, DocumentRecipe, EMBEDDING_READ_PAGE, EXACT_SEARCH_SLOTS,
    EXACT_WORKER_LIMIT, EmbeddingClient, EmbeddingDocumentQuery, EmbeddingVector, EngineError,
    FamilyCoverageSummary, NonZeroU32, OwnedSemaphorePermit, ResultPageRequest, ScoredThread,
    SearchHit, SearchMode, SearchProvenance, SearchRanking, SearchRequest, SearchResultPage,
    Semaphore, ThreadSort, merge_scored_pages, resolve_repositories, score_embedding_page,
    store_sort, store_state_filter,
};

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
    let mut cursor = None;
    let first_page = loop {
        let page = archive
            .embedding_search_page(&EmbeddingDocumentQuery {
                repositories: &repositories,
                kind: request.filters.kind,
                state: store_state_filter(request.filters.state),
                endpoint: client.endpoint_identity(),
                model: client.model(),
                recipe,
                after_document_id: cursor,
                limit,
            })
            .await?;
        let has_expected_dimension = page.items.iter().any(|document| {
            document.chunks.first().is_some_and(|first| {
                client
                    .dimensions()
                    .is_none_or(|dimensions| first.vector.dimensions() == dimensions)
            })
        });
        if has_expected_dimension {
            break page;
        }
        let Some(next_cursor) = page.next_document_id else {
            return Err(EngineError::SemanticVectorsUnavailable);
        };
        cursor = Some(next_cursor);
    };

    let query_text = request.query.trim().to_owned();
    let mut query_vectors = client.embed(&[query_text], cancellation).await?;
    let query_vector = query_vectors
        .pop()
        .ok_or(EngineError::EmbeddingServiceUnavailable)?;
    let sort = request.filters.sort.unwrap_or(ThreadSort::Relevance);
    let mut ranked = Vec::with_capacity(candidate_limit);
    let mut next_cursor = first_page.next_document_id;
    let mut compatible_documents =
        count_dimension_compatible(&first_page.items, query_vector.dimensions());
    let mut scored = score_page_bounded(
        query_vector.clone(),
        first_page.items,
        sort,
        candidate_limit,
        cancellation,
    )
    .await?;
    merge_scored_pages(
        &mut ranked,
        std::mem::take(&mut scored),
        store_sort(sort),
        candidate_limit,
    );

    while let Some(after_document_id) = next_cursor {
        if cancellation.is_cancelled() {
            return Err(EngineError::SearchCancelled);
        }
        let page = archive
            .embedding_search_page(&EmbeddingDocumentQuery {
                repositories: &repositories,
                kind: request.filters.kind,
                state: store_state_filter(request.filters.state),
                endpoint: client.endpoint_identity(),
                model: client.model(),
                recipe,
                after_document_id: Some(after_document_id),
                limit,
            })
            .await?;
        next_cursor = page.next_document_id;
        compatible_documents = compatible_documents.saturating_add(count_dimension_compatible(
            &page.items,
            query_vector.dimensions(),
        ));
        let page_scores = score_page_bounded(
            query_vector.clone(),
            page.items,
            sort,
            candidate_limit,
            cancellation,
        )
        .await?;
        merge_scored_pages(&mut ranked, page_scores, store_sort(sort), candidate_limit);
    }
    if compatible_documents == 0 {
        return Err(EngineError::SemanticVectorsUnavailable);
    }
    Ok(ranked)
}

/// Counts vectors matching the selected model and query dimension.
pub fn count_dimension_compatible(
    documents: &[forgesync_store::embeddings::EmbeddingSearchDocument],
    dimensions: u32,
) -> usize {
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
    documents: Vec<forgesync_store::embeddings::EmbeddingSearchDocument>,
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
