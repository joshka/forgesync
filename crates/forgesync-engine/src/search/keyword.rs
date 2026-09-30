//! # Build and retrieve keyword candidates
//!
//! Keyword helpers turn user text into a safe full-text expression and request candidate threads
//! from the archive. They also form result pages or a permitted fallback when semantic work cannot
//! complete.
//!
//! The store owns bound SQL and full-text storage. This module owns search interpretation and the
//! shape of keyword evidence used by `ranking`.

use forgesync_store::archive::Archive;
use forgesync_store::reads::ThreadPage;

use crate::error::EngineError;
use crate::inspect::{ThreadFilters, ThreadSort};
use crate::search::ranking::result_page;
use crate::search::{
    KeywordCandidates, ResultPageRequest, SearchHit, SearchMode, SearchProvenance, SearchRanking,
    SearchRequest, SearchResultPage, search_threads,
};

/// Collects local full-text candidates for hybrid rank fusion.
pub async fn keyword_candidates(
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

/// Wraps a keyword page with requested and effective mode metadata.
pub fn keyword_result_page(
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

/// Keeps the semantic failure reason when returning keyword candidates.
pub fn keyword_fallback_page(
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

/// Quotes ordinary search terms for the local FTS index.
pub fn keyword_expression(query: &str) -> Option<String> {
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
