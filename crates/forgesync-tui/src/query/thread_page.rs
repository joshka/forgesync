//! # Prepare one local discussion page
//!
//! [`ThreadRead`] binds the submitted keyword query, applied repository scope, and row offset.
//! Its [`ThreadRead::page`] method selects keyword ranking or ordinary update-order browsing;
//! the request owns filter construction so background task spawning need not interpret search
//! policy. Both paths select all discussion kinds and source states, with a hundred-row bound.
//!
//! Keyword reads use archived full-text data only. They request neither semantic query embeddings
//! nor a provider refresh. Browsing also stays local. Repository selectors are copied into the
//! engine's owned filter request at this boundary; their archive resolution belongs to the engine.
//! [`crate::query::reads`] starts the panel generation and sends the resulting page together with
//! this request's offset, keeping scheduling and page preparation independently readable.

use forgesync_engine::error::EngineError;
use forgesync_engine::inspect::{
    ThreadFilters, ThreadListRequest, ThreadSort, ThreadStateFilter, list_threads,
};
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::search::{SearchMode, SearchRequest, search_threads};
use forgesync_store::archive::Archive;
use forgesync_store::reads::ThreadPage;

/// Applied discussion scope and optional keyword query for one bounded terminal page.
pub struct ThreadRead {
    /// Submitted keyword query; absence selects update-order browsing.
    pub query: Option<String>,
    /// Applied repository scope; empty selects all archived repositories.
    pub repositories: Vec<RepositorySelector>,
    /// Zero-based offset carried back with the page to its list owner.
    pub offset: u64,
}

impl ThreadRead {
    /// Reads a local page using keyword relevance when a submitted query exists.
    ///
    /// Errors retain the engine's classification until the scheduling boundary formats them for
    /// terminal presentation. This method never changes panel state or performs provider I/O.
    pub async fn page(&self, archive: &Archive) -> Result<ThreadPage, EngineError> {
        match &self.query {
            Some(query) => self.keyword_page(archive, query).await,
            None => self.browser_page(archive).await,
        }
    }

    /// Searches archived full text using the same scope and pagination as ordinary browsing.
    async fn keyword_page(
        &self,
        archive: &Archive,
        query: &str,
    ) -> Result<ThreadPage, EngineError> {
        let request = SearchRequest {
            query: query.to_owned(),
            mode: SearchMode::Keyword,
            filters: self.filters(ThreadSort::Relevance),
            allow_keyword_fallback: false,
        };
        search_threads(archive, &request).await
    }

    /// Lists archived discussions by newest source update when no keyword query was submitted.
    async fn browser_page(&self, archive: &Archive) -> Result<ThreadPage, EngineError> {
        let request = ThreadListRequest {
            filters: self.filters(ThreadSort::Updated),
        };
        list_threads(archive, &request).await
    }

    /// Constructs the common scope once per read, leaving only ranking policy to the caller.
    fn filters(&self, sort: ThreadSort) -> ThreadFilters {
        ThreadFilters {
            repositories: self.repositories.clone(),
            kind: None,
            state: ThreadStateFilter::All,
            sort: Some(sort),
            limit: 100,
            offset: self.offset,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Both ranking policies retain the same bounded repository and pagination scope.

    use forgesync_engine::inspect::{ThreadSort, ThreadStateFilter};

    use crate::query::thread_page::ThreadRead;

    #[rstest::rstest]
    #[case::keyword_relevance(ThreadSort::Relevance)]
    #[case::ordinary_browsing(ThreadSort::Updated)]
    fn page_filters_preserve_scope_and_bounds(#[case] sort: ThreadSort) {
        let repository = "owner/repo".parse().expect("repository selector");
        let request = ThreadRead {
            query: None,
            repositories: vec![repository],
            offset: 200,
        };

        let filters = request.filters(sort);

        assert_eq!(filters.repositories, request.repositories);
        assert_eq!(filters.sort, Some(sort));
        assert_eq!(filters.kind, None);
        assert_eq!(filters.state, ThreadStateFilter::All);
        assert_eq!((filters.limit, filters.offset), (100, 200));
    }
}
