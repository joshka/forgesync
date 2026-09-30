//! # Build and retrieve keyword candidates
//!
//! Keyword helpers turn user text into a safe full-text expression and request candidate threads
//! from the archive. They also form result pages or a permitted fallback when semantic work cannot
//! complete.
//!
//! The store owns bound SQL and full-text storage. This module owns search interpretation and the
//! shape of keyword evidence used by `ranking`.
//!
//! [`keyword_expression`] treats punctuation as separators and quotes each surviving Unicode
//! alphanumeric/underscore term. It deliberately does not expose FTS operators from user text.
//! A punctuation-only query has no expression; the search coordinator decides how empty queries
//! behave rather than sending invalid FTS syntax to SQLite.
//!
//! [`keyword_candidates`] starts at offset zero to collect the prefix needed by hybrid fusion,
//! using at most 1,000 rows per read. It preserves local result order and attaches one-based
//! keyword ranks instead of inventing comparable numeric scores. Empty or nonadvancing pages end
//! retrieval defensively. Separate pages are separate reads, not a frozen database snapshot.
//!
//! [`keyword_result_page`] annotates an already paged store result. [`keyword_fallback_page`]
//! instead pages a previously acquired prefix while retaining the semantic failure explanation.
//! Neither helper decides whether fallback is permitted: that policy belongs to `ranking` and
//! the search coordinator. Coverage travels with the candidates and proves no new acquisition.

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
/// Retains query, repository, and state filters but replaces page coordinates for prefix
/// acquisition. Each successful page contributes one-based keyword provenance and no numeric
/// score. Coverage is retained from the first nonempty coverage report; zero requested candidates
/// perform no reads and return empty coverage. Reads across pages need not share a snapshot.
///
/// # Errors
///
/// Propagates search validation and store errors. An empty page or nonadvancing continuation ends
/// collection instead of retrying forever; it can return fewer than `count` candidates.
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
///
/// This private module's value owns accumulated hits rather than retrieval policy. It preserves
/// page order and accepts coverage from the first page that supplies any coverage entries.
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
    ///
    /// Members are retained even when continuation metadata cannot advance. This terminates
    /// retrieval defensively without discarding the successful page or retrying its offset.
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

/// Annotates an already paged keyword result without changing its order or continuation.
///
/// The request offset is the page's starting rank coordinate, so provenance is one-based across
/// pages. Oversized ranks saturate at `u32::MAX`. Requested and effective modes remain separate to
/// expose fallback: a supplied reason selects effective keyword mode. This does not authorize
/// fallback.
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

/// Pages a keyword prefix while retaining the permitted semantic failure explanation.
///
/// The caller has already checked fallback policy. Candidate order, rank provenance, and coverage
/// are preserved; the validated request coordinates select the slice through
/// `ranking::result_page`. The effective mode is keyword even when the requested mode was semantic
/// or hybrid.
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

/// Quotes ordinary terms so user text cannot become FTS operators.
///
/// Splits on every character except Unicode alphanumerics and underscores, discards empty pieces,
/// and joins individually quoted terms with spaces. Case and term order are preserved. Returns
/// `None` when no term survives. This is literal keyword interpretation rather than an FTS query
/// parser: `OR`, wildcards, parentheses, and `NEAR` receive no special operator meaning.
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
