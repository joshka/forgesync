//! Page ranked candidates and classify which semantic failures permit keyword fallback.
//!
//! Continuation describes whether more of the acquired prefix remains, not whether unseen archive
//! candidates exist beyond it.

use forgesync_store::reads::FamilyCoverageSummary;

use crate::error::EngineError;
use crate::inspect::ThreadSort;
use crate::search::{SearchHit, SearchMode, SearchRanking, SearchResultPage};

/// An already ordered candidate prefix plus the metadata reported with its page.
pub struct ResultPageRequest<'a> {
    /// Trimmed query text.
    pub query: &'a str,
    /// Mode selected by the caller before any permitted fallback.
    pub requested_mode: SearchMode,
    /// Mode that actually produced the candidate prefix.
    pub mode: SearchMode,
    /// Scoring/provenance strategy already applied to the candidates.
    pub ranking: SearchRanking,
    /// Reported sort policy; pagination does not reorder candidates.
    pub sort: ThreadSort,
    /// Safe failure code explaining a permitted keyword fallback, when used.
    pub fallback_reason: Option<String>,
    /// Acquired prefix in final result order, consumed by the selected page slice.
    pub candidates: Vec<SearchHit>,
    /// Validated zero-based position in the acquired prefix.
    pub offset: u64,
    /// Validated maximum number of visible items.
    pub limit: u32,
    /// Coverage reported with the page.
    pub coverage: Vec<FamilyCoverageSummary>,
}

/// Applies pagination after ranking so `next_offset` describes the ordered candidate set, not
/// the size of an intermediate keyword or vector batch.
pub fn result_page(request: ResultPageRequest<'_>) -> SearchResultPage {
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

/// Whether the failure may fall back to keyword results: missing vectors, a missing service, and
/// ordinary embedding failures may; cancellation and setup failures may not.
pub fn fallback_allowed(error: &EngineError) -> bool {
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
