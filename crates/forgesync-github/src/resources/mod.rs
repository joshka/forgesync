//! REST resource acquisition: one normalized page per call plus its continuation.
//!
//! A page result never implies a complete collection; the engine and store own that transition
//! once every page of a family has been applied.

use forgesync_core::content::Repository;
use forgesync_core::identity::ThreadId;
use reqwest::Url;

use crate::error::GitHubError;

/// One normalized page and the provider's next-page destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_page: Option<Url>,
}

/// Source states selected from the GitHub issues endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadListState {
    All,
    Open,
    Closed,
}

mod fetch;
mod normalize;
mod wire;

pub use fetch::{
    fetch_issue_comment_page, fetch_pull_request_metadata, fetch_pull_request_review_page,
    fetch_repository, fetch_thread_page_in_scope, thread_list_url_in_scope,
};

/// Rejects a thread whose repository identity differs from the selected repository.
pub(crate) fn require_thread_scope(
    repository: &Repository,
    thread: &ThreadId,
) -> Result<(), GitHubError> {
    if thread.repository() == &repository.id {
        Ok(())
    } else {
        Err(GitHubError::InvalidProviderData)
    }
}

#[cfg(test)]
mod tests;
