//! # Archived keyword, semantic, and hybrid search
//!
//! `SearchRequest` selects scope, mode, ranking, and pagination. `SearchHit` and
//! `SearchResultPage` include provenance so callers can distinguish keyword, semantic, and
//! combined evidence. `search_threads` and `retrieve_threads` are the engine entry points.
//!
//! `keyword` supplies text candidates, `semantic` scores compatible vectors, and `ranking`
//! pages results and classifies fallback, while `fusion` owns combined source ranks and provenance.
//! Search reads archived discussions and stored document vectors. Keyword search stays offline;
//! semantic and hybrid search send query text to the configured embedding service. They never
//! refresh source discussions or persist new document vectors. Mode and fallback policy remain
//! explicit so callers can explain availability and network use.

use forgesync_core::document::DocumentRecipe;
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use forgesync_store::reads::{FamilyCoverageSummary, ThreadPage, ThreadQuery, ThreadSummary};
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::embedding_client::EmbeddingClient;
use crate::error::EngineError;
use crate::inspect::{
    ThreadFilters, ThreadSort, checked_page, resolve_repositories, store_sort, store_state_filter,
};

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
///
/// [`SearchMode::Keyword`] quotes ordinary text as terms; [`SearchMode::AdvancedFts`] accepts an
/// FTS5 expression. Semantic and hybrid retrieval use [`retrieve_threads`] with an embedding
/// client and matching document recipe. Fallback must be requested explicitly and only applies
/// to semantic or hybrid modes.
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
///
/// Keyword modes use the local full-text index. Semantic and hybrid modes embed the query with
/// `embedding_client`, then rank compatible current document vectors. If semantic retrieval is
/// unavailable, `allow_keyword_fallback` controls whether the result reports a keyword fallback
/// or returns an error. Provider data is never fetched by this operation.
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
    match request.mode {
        SearchMode::Keyword | SearchMode::AdvancedFts => retrieve_keyword(archive, request).await,
        SearchMode::Semantic | SearchMode::Hybrid => {
            retrieve_ranked(archive, request, recipe, embedding_client, cancellation).await
        }
    }
}

/// Runs the local full-text path and marks keyword provenance in the result.
async fn retrieve_keyword(
    archive: &Archive,
    request: &SearchRequest,
) -> Result<SearchResultPage, EngineError> {
    let query = request.query.trim();
    let sort = request.filters.sort.unwrap_or(ThreadSort::Relevance);
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

/// Reuses hybrid keyword candidates or performs a local keyword search after semantic failure.
async fn retrieve_keyword_fallback(
    archive: &Archive,
    request: &SearchRequest,
    keyword_candidates: Option<KeywordCandidates>,
    error: &EngineError,
) -> Result<SearchResultPage, EngineError> {
    let query = request.query.trim();
    let sort = request.filters.sort.unwrap_or(ThreadSort::Relevance);
    let (limit, offset) = checked_page(request.filters.limit, request.filters.offset)?;
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
    Ok(keyword_result_page(
        query,
        request.mode,
        SearchMode::Keyword,
        sort,
        page,
        offset,
        Some(reason),
    ))
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

mod fusion;
mod keyword;
mod ranked;
mod ranking;
mod semantic;
mod window;

use keyword::{keyword_expression, keyword_fallback_page, keyword_result_page};
use ranked::retrieve_ranked;

#[cfg(test)]
mod tests {
    use crate::search::fusion::reciprocal_rank_score;

    #[test]
    fn hybrid_rank_uses_the_selected_constant_and_source_provenance() {
        assert!((reciprocal_rank_score(1) - (1.0 / 61.0)).abs() < f64::EPSILON);
    }
}
