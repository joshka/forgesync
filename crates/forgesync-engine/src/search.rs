use forgesync_store::{Archive, StoreError, ThreadPage, ThreadQuery};

use crate::EngineError;
use crate::inspect::{
    ThreadFilters, ThreadSort, checked_page, resolve_repositories, store_sort, store_state_filter,
};

/// Search syntax accepted by the offline keyword workflow.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SearchMode {
    /// Treat input as ordinary text and quote each token before sending it to FTS5.
    #[default]
    Keyword,
    /// Accept an explicit FTS5 expression, including boolean operators and phrases.
    AdvancedFts,
}

/// Request for a read-only local keyword search.
#[derive(Clone, Debug)]
pub struct SearchRequest {
    /// User-supplied query text.
    pub query: String,
    /// Select ordinary token search or explicit advanced FTS5 syntax.
    pub mode: SearchMode,
    /// Repository and discussion filters with pagination.
    pub filters: ThreadFilters,
}

/// Searches archived discussions without network access or archive writes.
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
    }
    .unwrap_or_default();
    let (limit, offset) = checked_page(request.filters.limit, request.filters.offset)?;
    let repositories = resolve_repositories(archive, &request.filters.repositories).await?;
    let query = ThreadQuery {
        repositories,
        kind: request.filters.kind,
        state: store_state_filter(request.filters.state),
        match_expression: Some(match_expression),
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

fn keyword_expression(query: &str) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::keyword_expression;

    #[test]
    fn ordinary_text_becomes_quoted_terms_instead_of_fts_syntax() {
        assert_eq!(
            keyword_expression("issues OR (cache* NEAR/4 timeout)"),
            Some("\"issues\" \"OR\" \"cache\" \"NEAR\" \"4\" \"timeout\"".to_owned())
        );
        assert_eq!(keyword_expression("***"), None);
    }
}
