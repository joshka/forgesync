//! # Combine candidate lists into search results
//!
//! Ranking helpers page scored candidates, fuse hybrid lists, and assign reciprocal-rank
//! contributions. They also decide when a semantic failure permits a keyword fallback.
//!
//! Candidate generation lives in `keyword` and `semantic`; this module owns the cross-mode
//! ordering visible to users. The result keeps provenance so a caller can explain why a hit
//! appeared.

use crate::error::EngineError;
use crate::search::{ResultPageRequest, SearchResultPage};

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

/// Limits keyword fallback to semantic failures that can be explained safely.
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
