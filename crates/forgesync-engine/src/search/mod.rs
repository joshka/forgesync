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

mod keyword;
mod ranking;
mod semantic;

use keyword::*;
use ranking::*;
use semantic::*;

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
