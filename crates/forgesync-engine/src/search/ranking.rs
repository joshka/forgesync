//! # Combine candidate lists into search results
//!
//! Ranking helpers page scored candidates and decide when a semantic failure permits a keyword
//! fallback. Hybrid source merging and reciprocal-rank contributions live in `fusion`.
//!
//! Candidate generation lives in `keyword` and `semantic`; this module owns the cross-mode
//! ordering visible to users. The result keeps provenance so a caller can explain why a hit
//! appeared.
//!
//! [`result_page`] consumes a candidate vector whose order is already established. It does not
//! score or sort it: semantic ordering and hybrid fusion happen before this boundary. Continuation
//! describes whether more of that acquired prefix remains, not whether unseen archive candidates
//! exist beyond it. Request metadata and coverage are carried into the result unchanged.
//!
//! [`fallback_allowed`] classifies failures only. Missing vectors or service availability can
//! permit a keyword result, but cancellation, configuration, and concurrency setup failures must
//! remain failures. The coordinator additionally checks the caller's explicit fallback preference;
//! this module never initiates provider calls or mutates archive state.

use forgesync_store::reads::FamilyCoverageSummary;

use crate::error::EngineError;
use crate::inspect::ThreadSort;
use crate::search::{SearchHit, SearchMode, SearchRanking, SearchResultPage};

/// Prepared ordered prefix and metadata consumed by final result pagination.
///
/// Candidate generation and fusion establish order before constructing this value. It carries
/// projection facts rather than an executable user request: effective mode, ranking provenance,
/// and fallback reason can differ from what the caller originally requested. Coordinates have
/// already been validated by the workflow; construction performs no validation or archive I/O.
pub struct ResultPageRequest<'a> {
    /// Trimmed query text retained for the visible result, not interpreted here.
    pub query: &'a str,
    /// Mode selected by the caller before any permitted fallback.
    pub requested_mode: SearchMode,
    /// Mode that actually produced the candidate prefix.
    pub mode: SearchMode,
    /// Scoring/provenance strategy already applied to the candidates.
    pub ranking: SearchRanking,
    /// Reported sort policy; pagination does not reorder candidates to enforce it.
    pub sort: ThreadSort,
    /// Safe failure code explaining a permitted keyword fallback, when used.
    pub fallback_reason: Option<String>,
    /// Acquired prefix in final result order, consumed by the selected page slice.
    pub candidates: Vec<SearchHit>,
    /// Validated zero-based position in the acquired prefix.
    pub offset: u64,
    /// Validated maximum number of visible items.
    pub limit: u32,
    /// Coverage observation retained with this projection, without freshness mutation.
    pub coverage: Vec<FamilyCoverageSummary>,
}

/// Applies pagination after ranking so `next_offset` describes the ordered candidate set, not
/// the size of an intermediate keyword or vector batch.
///
/// Preserves supplied candidate order and metadata. Coordinates are expected to have been checked
/// by the search window; this projection does not validate them. An offset beyond the prefix
/// returns no items or continuation. Continuation addition is checked and omitted on overflow.
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

/// Reports whether the failure category permits considering keyword fallback.
///
/// Missing vectors, a missing service, and ordinary embedding failures are eligible. Cancellation,
/// invalid service configuration, concurrency setup failure, and unrelated engine errors are not.
/// Eligibility does not override the request's fallback preference and performs no retrieval.
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
