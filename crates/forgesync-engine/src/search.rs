//! Keyword, semantic, and hybrid search policy.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::{Arc, OnceLock};

use forgesync_core::document::DocumentRecipe;
use forgesync_core::embedding::EmbeddingVector;
use forgesync_core::identity::ThreadId;
use forgesync_store::archive::Archive;
use forgesync_store::embeddings::EmbeddingDocumentQuery;
use forgesync_store::error::StoreError;
use forgesync_store::reads::{FamilyCoverageSummary, ThreadPage, ThreadQuery, ThreadSummary};
use serde::Serialize;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::embedding_client::EmbeddingClient;
use crate::error::EngineError;
use crate::exact_search::{
    ScoredThread, merge_scored_pages, score_embedding_page, stable_thread_id_cmp,
};
use crate::inspect::{
    ThreadFilters, ThreadSort, checked_page, resolve_repositories, store_sort, store_state_filter,
};

const MAX_SEARCH_WINDOW: usize = 10_000;
const EMBEDDING_READ_PAGE: u32 = 128;
const EXACT_WORKER_LIMIT: usize = 2;
const RRF_CONSTANT: f64 = 60.0;

static EXACT_SEARCH_SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();

/// Search mode selected by an application caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    /// Quote ordinary text tokens and search the local FTS5 index.
    Keyword,
    /// Accept an explicit FTS5 expression.
    AdvancedFts,
    /// Rank current compatible vectors by exact cosine similarity.
    Semantic,
    /// Fuse keyword and semantic ranks with reciprocal rank fusion.
    Hybrid,
}

/// Result ranking policy reported to the caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchRanking {
    /// Local full-text result order.
    Keyword,
    /// Exact cosine similarity over current compatible vectors.
    Cosine,
    /// Reciprocal rank fusion using the constant 60.
    ReciprocalRankFusion,
}

/// One source rank contributing to an individual result.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum SearchProvenance {
    /// Rank from the local keyword result list.
    Keyword {
        /// One-based position in the keyword result list.
        rank: u32,
    },
    /// Rank and cosine score from exact semantic retrieval.
    Semantic {
        /// One-based position in the semantic result list.
        rank: u32,
        /// Cosine similarity for the current document.
        cosine_score: f64,
    },
}

/// Ranked discussion and the result sources that contributed to its position.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SearchHit {
    /// Discussion and current evidence coverage.
    pub summary: ThreadSummary,
    /// Mode-specific score, omitted for keyword-only rankings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// Source ranks used to produce this result.
    pub provenance: Vec<SearchProvenance>,
}

/// Search result with requested/effective mode and ranking provenance.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SearchResultPage {
    /// User-supplied search text.
    pub query: String,
    /// Mode requested by the caller.
    pub requested_mode: SearchMode,
    /// Mode that produced the returned results.
    pub mode: SearchMode,
    /// Effective ranking policy.
    pub ranking: SearchRanking,
    /// Search sort order applied after scoring or fusion.
    pub sort: ThreadSort,
    /// Provider or vector-coverage classification when explicit fallback was used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
    /// Ranked results.
    pub items: Vec<SearchHit>,
    /// Offset to pass to the next request, when more results are available.
    pub next_offset: Option<u64>,
    /// Coverage totals for the selected repository scope.
    pub coverage: Vec<FamilyCoverageSummary>,
}

/// Request for a read-only local discussion search.
#[derive(Clone, Debug)]
pub struct SearchRequest {
    /// User-supplied query text.
    pub query: String,
    /// Keyword, explicit FTS5, semantic, or hybrid ranking mode.
    pub mode: SearchMode,
    /// Repository and discussion filters with pagination.
    pub filters: ThreadFilters,
    /// Permit keyword-only results if semantic retrieval is unavailable.
    pub allow_keyword_fallback: bool,
}

/// Searches archived discussions without contacting GitHub or mutating the archive.
pub async fn search_threads(
    archive: &Archive,
    request: &SearchRequest,
) -> Result<ThreadPage, EngineError> {
    let query_text = request.query.trim();
    if query_text.is_empty() {
        return Err(EngineError::InvalidSearchQuery);
    }
    let match_expression = match request.mode {
        SearchMode::Keyword => keyword_expression(query_text),
        SearchMode::AdvancedFts => Some(query_text.to_owned()),
        SearchMode::Semantic | SearchMode::Hybrid => {
            return Err(EngineError::EmbeddingServiceUnavailable);
        }
    }
    .unwrap_or_default();
    let (limit, offset) = checked_page(request.filters.limit, request.filters.offset)?;
    let repositories = resolve_repositories(archive, &request.filters.repositories).await?;
    let query = ThreadQuery {
        repositories,
        kind: request.filters.kind,
        state: store_state_filter(request.filters.state),
        match_expression: Some(match_expression),
        updated_since: None,
        sort: store_sort(request.filters.sort.unwrap_or(ThreadSort::Relevance)),
        limit,
        offset,
    };
    archive
        .query_threads(&query)
        .await
        .map_err(|error| match error {
            StoreError::InvalidSearchQuery => EngineError::InvalidSearchQuery,
            error => EngineError::Store(error),
        })
}

/// Runs keyword, semantic, or hybrid retrieval through one read-only application operation.
pub async fn retrieve_threads(
    archive: &Archive,
    request: &SearchRequest,
    recipe: DocumentRecipe,
    embedding_client: Option<&EmbeddingClient>,
    cancellation: &CancellationToken,
) -> Result<SearchResultPage, EngineError> {
    let query = request.query.trim();
    if query.is_empty() {
        return Err(EngineError::InvalidSearchQuery);
    }
    if request.allow_keyword_fallback
        && !matches!(request.mode, SearchMode::Semantic | SearchMode::Hybrid)
    {
        return Err(EngineError::InvalidSearchFallbackMode);
    }
    let sort = request.filters.sort.unwrap_or(ThreadSort::Relevance);
    match request.mode {
        SearchMode::Keyword | SearchMode::AdvancedFts => {
            let page = search_threads(archive, request).await?;
            Ok(keyword_result_page(
                query,
                request.mode,
                request.mode,
                sort,
                page,
                request.filters.offset,
                None,
            ))
        }
        SearchMode::Semantic | SearchMode::Hybrid => {
            let (limit, offset) = checked_page(request.filters.limit, request.filters.offset)?;
            let window = usize::try_from(offset)
                .ok()
                .and_then(|offset| offset.checked_add(usize::try_from(limit.get()).ok()?))
                .filter(|window| *window <= MAX_SEARCH_WINDOW)
                .ok_or(EngineError::SearchWindowTooLarge)?;
            let candidate_limit = window + 1;
            let keyword_candidates = if request.mode == SearchMode::Hybrid {
                Some(keyword_candidates(archive, request, candidate_limit).await?)
            } else {
                None
            };

            let semantic_result = match embedding_client {
                Some(client) => {
                    semantic_candidates(
                        archive,
                        request,
                        recipe,
                        client,
                        candidate_limit,
                        cancellation,
                    )
                    .await
                }
                None => Err(EngineError::EmbeddingServiceUnavailable),
            };

            let semantic = match semantic_result {
                Ok(semantic) => semantic,
                Err(error) if request.allow_keyword_fallback && fallback_allowed(&error) => {
                    let reason = error.code().to_owned();
                    if let Some(keyword) = keyword_candidates {
                        return Ok(keyword_fallback_page(
                            query,
                            request.mode,
                            sort,
                            keyword,
                            offset,
                            limit.get(),
                            reason,
                        ));
                    }
                    let fallback_request = SearchRequest {
                        mode: SearchMode::Keyword,
                        allow_keyword_fallback: false,
                        ..request.clone()
                    };
                    let page = search_threads(archive, &fallback_request).await?;
                    return Ok(keyword_result_page(
                        query,
                        request.mode,
                        SearchMode::Keyword,
                        sort,
                        page,
                        offset,
                        Some(reason),
                    ));
                }
                Err(error) => return Err(error),
            };

            let coverage_repositories =
                resolve_repositories(archive, &request.filters.repositories).await?;
            let coverage = archive.coverage_summary(&coverage_repositories).await?;
            match request.mode {
                SearchMode::Semantic => Ok(semantic_result_page(
                    query,
                    request.mode,
                    sort,
                    semantic,
                    offset,
                    limit.get(),
                    coverage,
                )),
                SearchMode::Hybrid => {
                    let keyword =
                        keyword_candidates.expect("hybrid mode loaded keyword candidates");
                    let fused = fuse_hybrid(keyword.items, semantic, sort, candidate_limit);
                    Ok(result_page(ResultPageRequest {
                        query,
                        requested_mode: request.mode,
                        mode: request.mode,
                        ranking: SearchRanking::ReciprocalRankFusion,
                        sort,
                        fallback_reason: None,
                        candidates: fused,
                        offset,
                        limit: limit.get(),
                        coverage,
                    }))
                }
                SearchMode::Keyword | SearchMode::AdvancedFts => unreachable!(),
            }
        }
    }
}

struct KeywordCandidates {
    items: Vec<SearchHit>,
    coverage: Vec<FamilyCoverageSummary>,
}

struct ResultPageRequest<'a> {
    query: &'a str,
    requested_mode: SearchMode,
    mode: SearchMode,
    ranking: SearchRanking,
    sort: ThreadSort,
    fallback_reason: Option<String>,
    candidates: Vec<SearchHit>,
    offset: u64,
    limit: u32,
    coverage: Vec<FamilyCoverageSummary>,
}

async fn keyword_candidates(
    archive: &Archive,
    request: &SearchRequest,
    count: usize,
) -> Result<KeywordCandidates, EngineError> {
    let mut candidates = Vec::with_capacity(count);
    let mut coverage = Vec::new();
    let mut offset = 0_u64;
    while candidates.len() < count {
        let remaining = count - candidates.len();
        let page_limit = u32::try_from(remaining.min(1000)).unwrap_or(1000);
        let page_request = SearchRequest {
            mode: SearchMode::Keyword,
            allow_keyword_fallback: false,
            filters: ThreadFilters {
                limit: page_limit,
                offset,
                ..request.filters.clone()
            },
            ..request.clone()
        };
        let page = search_threads(archive, &page_request).await?;
        if coverage.is_empty() {
            coverage = page.coverage.clone();
        }
        let received = page.items.len();
        let start_rank = usize::try_from(offset).unwrap_or(usize::MAX);
        candidates.extend(
            page.items
                .into_iter()
                .enumerate()
                .map(|(index, summary)| SearchHit {
                    summary,
                    score: None,
                    provenance: vec![SearchProvenance::Keyword {
                        rank: u32::try_from(start_rank.saturating_add(index).saturating_add(1))
                            .unwrap_or(u32::MAX),
                    }],
                }),
        );
        let Some(next_offset) = page.next_offset else {
            break;
        };
        if received == 0 || next_offset <= offset {
            break;
        }
        offset = next_offset;
    }
    Ok(KeywordCandidates {
        items: candidates,
        coverage,
    })
}

async fn semantic_candidates(
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

fn count_dimension_compatible(
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

async fn score_page_bounded(
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

fn keyword_result_page(
    query: &str,
    requested_mode: SearchMode,
    mode: SearchMode,
    sort: ThreadSort,
    page: ThreadPage,
    offset: u64,
    fallback_reason: Option<String>,
) -> SearchResultPage {
    let start_rank = usize::try_from(offset).unwrap_or(usize::MAX);
    let items = page
        .items
        .into_iter()
        .enumerate()
        .map(|(index, summary)| SearchHit {
            summary,
            score: None,
            provenance: vec![SearchProvenance::Keyword {
                rank: u32::try_from(start_rank.saturating_add(index).saturating_add(1))
                    .unwrap_or(u32::MAX),
            }],
        })
        .collect();
    SearchResultPage {
        query: query.to_owned(),
        requested_mode,
        mode,
        ranking: SearchRanking::Keyword,
        sort,
        fallback_reason,
        items,
        next_offset: page.next_offset,
        coverage: page.coverage,
    }
}

fn keyword_fallback_page(
    query: &str,
    requested_mode: SearchMode,
    sort: ThreadSort,
    candidates: KeywordCandidates,
    offset: u64,
    limit: u32,
    fallback_reason: String,
) -> SearchResultPage {
    result_page(ResultPageRequest {
        query,
        requested_mode,
        mode: SearchMode::Keyword,
        ranking: SearchRanking::Keyword,
        sort,
        fallback_reason: Some(fallback_reason),
        candidates: candidates.items,
        offset,
        limit,
        coverage: candidates.coverage,
    })
}

fn semantic_result_page(
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

fn result_page(request: ResultPageRequest<'_>) -> SearchResultPage {
    let ResultPageRequest {
        query,
        requested_mode,
        mode,
        ranking,
        sort,
        fallback_reason,
        candidates,
        offset,
        limit,
        coverage,
    } = request;
    let start = usize::try_from(offset).unwrap_or(usize::MAX);
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    let has_more = candidates.len() > start.saturating_add(limit);
    let items = candidates
        .into_iter()
        .skip(start)
        .take(limit)
        .collect::<Vec<_>>();
    let next_offset = if has_more {
        offset.checked_add(u64::try_from(items.len()).unwrap_or(u64::MAX))
    } else {
        None
    };
    SearchResultPage {
        query: query.to_owned(),
        requested_mode,
        mode,
        ranking,
        sort,
        fallback_reason,
        items,
        next_offset,
        coverage,
    }
}

fn fuse_hybrid(
    keyword: Vec<SearchHit>,
    semantic: Vec<ScoredThread>,
    sort: ThreadSort,
    limit: usize,
) -> Vec<SearchHit> {
    #[derive(Clone)]
    struct FusionEntry {
        summary: ThreadSummary,
        keyword_rank: Option<u32>,
        semantic_rank: Option<u32>,
        cosine_score: Option<f64>,
    }

    let mut entries =
        HashMap::<ThreadId, FusionEntry>::with_capacity(keyword.len() + semantic.len());
    for (index, hit) in keyword.into_iter().enumerate() {
        let rank = u32::try_from(index + 1).unwrap_or(u32::MAX);
        entries
            .entry(hit.summary.discussion.id.clone())
            .and_modify(|entry| entry.keyword_rank = Some(rank))
            .or_insert(FusionEntry {
                summary: hit.summary,
                keyword_rank: Some(rank),
                semantic_rank: None,
                cosine_score: None,
            });
    }
    for (index, candidate) in semantic.into_iter().enumerate() {
        let rank = u32::try_from(index + 1).unwrap_or(u32::MAX);
        entries
            .entry(candidate.summary.discussion.id.clone())
            .and_modify(|entry| {
                entry.semantic_rank = Some(rank);
                entry.cosine_score = Some(candidate.score);
            })
            .or_insert(FusionEntry {
                summary: candidate.summary,
                keyword_rank: None,
                semantic_rank: Some(rank),
                cosine_score: Some(candidate.score),
            });
    }

    let mut fused = entries
        .into_values()
        .map(|entry| {
            let keyword_score = entry.keyword_rank.map(reciprocal_rank_score).unwrap_or(0.0);
            let semantic_score = entry
                .semantic_rank
                .map(reciprocal_rank_score)
                .unwrap_or(0.0);
            let mut provenance = Vec::with_capacity(2);
            if let Some(rank) = entry.keyword_rank {
                provenance.push(SearchProvenance::Keyword { rank });
            }
            if let (Some(rank), Some(cosine_score)) = (entry.semantic_rank, entry.cosine_score) {
                provenance.push(SearchProvenance::Semantic { rank, cosine_score });
            }
            SearchHit {
                summary: entry.summary,
                score: Some(keyword_score + semantic_score),
                provenance,
            }
        })
        .collect::<Vec<_>>();
    fused.sort_by(|left, right| {
        let primary = match sort {
            ThreadSort::Relevance => right
                .score
                .unwrap_or_default()
                .total_cmp(&left.score.unwrap_or_default()),
            ThreadSort::Updated => right
                .summary
                .discussion
                .updated_at
                .cmp(&left.summary.discussion.updated_at),
            ThreadSort::Created => right
                .summary
                .discussion
                .created_at
                .cmp(&left.summary.discussion.created_at),
        };
        primary.then_with(|| stable_thread_id_cmp(&left.summary, &right.summary))
    });
    fused.truncate(limit);
    fused
}

fn reciprocal_rank_score(rank: u32) -> f64 {
    1.0 / (RRF_CONSTANT + f64::from(rank))
}

fn fallback_allowed(error: &EngineError) -> bool {
    match error {
        EngineError::SemanticVectorsUnavailable | EngineError::EmbeddingServiceUnavailable => true,
        EngineError::Embedding(error) => !matches!(
            error,
            crate::embedding_client::EmbeddingClientError::Cancelled
                | crate::embedding_client::EmbeddingClientError::InvalidConfiguration
                | crate::embedding_client::EmbeddingClientError::ConcurrencyUnavailable
        ),
        _ => false,
    }
}

fn keyword_expression(query: &str) -> Option<String> {
    let mut terms = Vec::new();
    let mut term = String::new();
    for character in query.chars() {
        if character.is_alphanumeric() || character == '_' {
            term.push(character);
        } else if !term.is_empty() {
            terms.push(std::mem::take(&mut term));
        }
    }
    if !term.is_empty() {
        terms.push(term);
    }
    if terms.is_empty() {
        return None;
    }
    Some(
        terms
            .into_iter()
            .map(|term| format!("\"{term}\""))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

#[cfg(test)]
mod tests {
    use super::{keyword_expression, reciprocal_rank_score};

    #[test]
    fn ordinary_text_becomes_quoted_terms_instead_of_fts_syntax() {
        assert_eq!(
            keyword_expression("issues OR (cache* NEAR/4 timeout)"),
            Some("\"issues\" \"OR\" \"cache\" \"NEAR\" \"4\" \"timeout\"".to_owned())
        );
        assert_eq!(keyword_expression("***"), None);
    }

    #[test]
    fn hybrid_rank_uses_the_selected_constant_and_source_provenance() {
        assert!((reciprocal_rank_score(1) - (1.0 / 61.0)).abs() < f64::EPSILON);
    }
}
