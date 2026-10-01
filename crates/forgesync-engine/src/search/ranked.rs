//! Coordinate semantic and hybrid retrieval.
//!
//! Hybrid retrieval loads keyword candidates first so permitted fallback can reuse that work.
//! Provider failures pass through the explicit fallback policy before any page is built.

use forgesync_core::document::DocumentRecipe;
use forgesync_store::archive::Archive;
use forgesync_store::reads::FamilyCoverageSummary;
use tokio_util::sync::CancellationToken;

use crate::embedding_client::EmbeddingClient;
use crate::error::EngineError;
use crate::inspect::ThreadSort;
use crate::query::resolve_repositories;
use crate::scoring::ScoredThread;
use crate::search::fusion::fuse_hybrid;
use crate::search::keyword::{KeywordCandidates, keyword_candidates};
use crate::search::ranking::{ResultPageRequest, fallback_allowed, result_page};
use crate::search::semantic::{semantic_candidates, semantic_result_page};
use crate::search::window::SearchWindow;
use crate::search::{
    SearchMode, SearchRanking, SearchRequest, SearchResultPage, retrieve_keyword_fallback,
};

/// Runs vector retrieval, optional keyword fusion, and explicit fallback policy.
pub async fn retrieve_ranked(
    archive: &Archive,
    request: &SearchRequest,
    recipe: DocumentRecipe,
    embedding_client: Option<&EmbeddingClient>,
    cancellation: &CancellationToken,
) -> Result<SearchResultPage, EngineError> {
    let window = SearchWindow::new(request.filters.limit, request.filters.offset)?;
    let search = RankedSearch { request, window };
    let keyword_candidates = if request.mode == SearchMode::Hybrid {
        Some(keyword_candidates(archive, request, window.candidate_limit).await?)
    } else {
        None
    };

    let semantic_result = search
        .candidates(archive, recipe, embedding_client, cancellation)
        .await;

    let semantic = match semantic_result {
        Ok(semantic) => semantic,
        Err(error) if request.allow_keyword_fallback && fallback_allowed(&error) => {
            return retrieve_keyword_fallback(archive, request, keyword_candidates, &error).await;
        }
        Err(error) => return Err(error),
    };

    let coverage_repositories =
        resolve_repositories(archive, &request.filters.repositories).await?;
    let coverage = archive.coverage_summary(&coverage_repositories).await?;
    Ok(search.project(keyword_candidates, semantic, coverage))
}

/// Validated request interpretation shared by candidate acquisition and result projection.
struct RankedSearch<'a> {
    /// Original mode, trimmed query source, and requested sort policy.
    request: &'a SearchRequest,
    /// Validated page coordinates and acquisition prefix.
    window: SearchWindow,
}

impl RankedSearch<'_> {
    /// Requires an embedding service and scores the candidate prefix for this validated window.
    async fn candidates(
        &self,
        archive: &Archive,
        recipe: DocumentRecipe,
        client: Option<&EmbeddingClient>,
        cancellation: &CancellationToken,
    ) -> Result<Vec<ScoredThread>, EngineError> {
        let client = client.ok_or(EngineError::EmbeddingServiceUnavailable)?;
        semantic_candidates(
            archive,
            self.request,
            recipe,
            client,
            self.window.candidate_limit,
            cancellation,
        )
        .await
    }

    /// Fuses with keyword candidates when hybrid retrieval loaded them, else projects cosine hits.
    fn project(
        self,
        keyword: Option<KeywordCandidates>,
        semantic: Vec<ScoredThread>,
        coverage: Vec<FamilyCoverageSummary>,
    ) -> SearchResultPage {
        match keyword {
            Some(keyword) => self.hybrid(keyword, semantic, coverage),
            None => self.semantic(semantic, coverage),
        }
    }

    /// Preserves cosine scores and semantic provenance before applying the page window.
    fn semantic(
        self,
        candidates: Vec<ScoredThread>,
        coverage: Vec<FamilyCoverageSummary>,
    ) -> SearchResultPage {
        semantic_result_page(self.request, candidates, coverage)
    }

    /// Combines both source ranks before pagination, retaining stable requested ordering.
    fn hybrid(
        self,
        keyword: KeywordCandidates,
        semantic: Vec<ScoredThread>,
        coverage: Vec<FamilyCoverageSummary>,
    ) -> SearchResultPage {
        let sort = self.request.filters.sort.unwrap_or(ThreadSort::Relevance);
        let candidates = fuse_hybrid(keyword.items, semantic, sort, self.window.candidate_limit);
        result_page(ResultPageRequest {
            query: self.request.query.trim(),
            requested_mode: self.request.mode,
            mode: self.request.mode,
            ranking: SearchRanking::ReciprocalRankFusion,
            sort,
            fallback_reason: None,
            candidates,
            offset: self.window.offset,
            limit: self.window.limit,
            coverage,
        })
    }
}
