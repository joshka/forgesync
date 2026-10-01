//! Build and retrieve keyword candidates.
//!
//! User text never exposes FTS operators: punctuation separates terms and each Unicode
//! alphanumeric/underscore term is quoted. Keyword results carry one-based ranks rather than
//! invented comparable scores.

use forgesync_store::archive::Archive;
use forgesync_store::reads::{FamilyCoverageSummary, ThreadPage, ThreadSummary};

use crate::error::EngineError;
use crate::inspect::{ThreadFilters, ThreadSort};
use crate::search::ranking::{ResultPageRequest, result_page};
use crate::search::{
    SearchHit, SearchMode, SearchProvenance, SearchRanking, SearchRequest, SearchResultPage,
    search_threads,
};

/// Collects up to `count` keyword hits from the beginning of the local result order.
///
/// An empty page or nonadvancing continuation ends collection rather than retrying forever.
pub async fn keyword_candidates(
    archive: &Archive,
    request: &SearchRequest,
    count: usize,
) -> Result<KeywordCandidates, EngineError> {
    let mut candidates = KeywordCandidates::new(count);
    let mut offset = 0_u64;
    while candidates.items.len() < count {
        let remaining = count - candidates.items.len();
        let page_request = candidate_page_request(request, remaining, offset);
        let page = search_threads(archive, &page_request).await?;
        let Some(next_offset) = candidates.append_page(page, offset) else {
            break;
        };
        offset = next_offset;
    }
    Ok(candidates)
}

/// Builds a keyword-only page for the remaining prefix, retaining the original scope filters.
fn candidate_page_request(request: &SearchRequest, remaining: usize, offset: u64) -> SearchRequest {
    let page_limit = u32::try_from(remaining.min(1000)).unwrap_or(1000);
    SearchRequest {
        mode: SearchMode::Keyword,
        allow_keyword_fallback: false,
        filters: ThreadFilters {
            limit: page_limit,
            offset,
            ..request.filters.clone()
        },
        ..request.clone()
    }
}

/// Keyword prefix and its reported coverage, retained for fusion or permitted fallback.
pub struct KeywordCandidates {
    /// Ordered prefix with one-based keyword provenance and no numeric score.
    pub items: Vec<SearchHit>,
    /// First nonempty page coverage, which need not share a snapshot with later hits.
    pub coverage: Vec<FamilyCoverageSummary>,
}

impl KeywordCandidates {
    /// Reserves the requested prefix capacity without reading the archive.
    fn new(count: usize) -> Self {
        Self {
            items: Vec::with_capacity(count),
            coverage: Vec::new(),
        }
    }

    /// Appends one page and returns only a nonempty, forward-moving continuation.
    fn append_page(&mut self, page: ThreadPage, offset: u64) -> Option<u64> {
        if self.coverage.is_empty() {
            self.coverage = page.coverage;
        }
        let received = page.items.len();
        self.items.extend(keyword_hits(page.items, offset));
        let next_offset = page.next_offset?;
        if received == 0 || next_offset <= offset {
            return None;
        }
        Some(next_offset)
    }
}

/// Annotates an already paged keyword result; ranks continue from the request offset, and a
/// fallback reason marks the effective mode as keyword.
pub fn keyword_result_page(
    request: &SearchRequest,
    page: ThreadPage,
    fallback_reason: Option<String>,
) -> SearchResultPage {
    let items = keyword_hits(page.items, request.filters.offset);
    let mode = if fallback_reason.is_some() {
        SearchMode::Keyword
    } else {
        request.mode
    };
    SearchResultPage {
        query: request.query.trim().to_owned(),
        requested_mode: request.mode,
        mode,
        ranking: SearchRanking::Keyword,
        sort: request.filters.sort.unwrap_or(ThreadSort::Relevance),
        fallback_reason,
        items,
        next_offset: page.next_offset,
        coverage: page.coverage,
    }
}

/// Projects local summaries into hits with one-based, saturating keyword ranks.
fn keyword_hits(summaries: Vec<ThreadSummary>, offset: u64) -> Vec<SearchHit> {
    let start_rank = usize::try_from(offset).unwrap_or(usize::MAX);
    summaries
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
        .collect()
}

/// Pages an acquired keyword prefix as a fallback, retaining the semantic failure explanation.
pub fn keyword_fallback_page(
    request: &SearchRequest,
    candidates: KeywordCandidates,
    fallback_reason: String,
) -> SearchResultPage {
    result_page(ResultPageRequest {
        query: request.query.trim(),
        requested_mode: request.mode,
        mode: SearchMode::Keyword,
        ranking: SearchRanking::Keyword,
        sort: request.filters.sort.unwrap_or(ThreadSort::Relevance),
        fallback_reason: Some(fallback_reason),
        candidates: candidates.items,
        offset: request.filters.offset,
        limit: request.filters.limit,
        coverage: candidates.coverage,
    })
}

/// Quotes ordinary terms so user text cannot become FTS operators (`OR`, wildcards, `NEAR`).
///
/// Returns `None` when no Unicode alphanumeric/underscore term survives.
pub fn keyword_expression(query: &str) -> Option<String> {
    let terms: Vec<_> = query
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{term}\""))
        .collect();
    if terms.is_empty() {
        return None;
    }
    Some(terms.join(" "))
}

#[cfg(test)]
mod tests {
    //! Literal keyword interpretation at the owning boundary.
    //!
    //! These linear examples make operator-looking text, punctuation-only input, and retained
    //! Unicode/underscore terms explicit. The store integration suites exercise actual FTS
    //! matching; these tests establish only the expression passed to that boundary.

    use crate::search::keyword::keyword_expression;

    #[test]
    fn operator_looking_text_becomes_quoted_terms() {
        let expression = keyword_expression("issues OR (cache* NEAR/4 timeout)");
        assert_eq!(
            expression,
            Some("\"issues\" \"OR\" \"cache\" \"NEAR\" \"4\" \"timeout\"".to_owned())
        );
    }

    #[test]
    fn punctuation_only_input_has_no_expression() {
        let expression = keyword_expression("***");
        assert_eq!(expression, None);
    }

    #[test]
    fn unicode_and_underscore_terms_keep_their_spelling() {
        let expression = keyword_expression("  café/cache_key---東京  ");
        assert_eq!(
            expression,
            Some("\"café\" \"cache_key\" \"東京\"".to_owned())
        );
    }

    #[test]
    fn empty_input_has_no_expression() {
        let expression = keyword_expression("");
        assert_eq!(expression, None);
    }
}
