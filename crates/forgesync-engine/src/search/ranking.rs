//! Ranking search behavior.

use super::{
    EngineError, HashMap, RRF_CONSTANT, ResultPageRequest, ScoredThread, SearchHit,
    SearchProvenance, SearchResultPage, ThreadId, ThreadSort, ThreadSummary, stable_thread_id_cmp,
};

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

/// Combines keyword and semantic ranks by stable thread identity. Keeping each source rank in
/// provenance lets a caller explain a fused hit without rerunning either search.
pub fn fuse_hybrid(
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

pub fn reciprocal_rank_score(rank: u32) -> f64 {
    1.0 / (RRF_CONSTANT + f64::from(rank))
}

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
