//! REST resource families and their page-oriented acquisition results.
//!
//! The public fetch functions in `fetch` request repositories, issue and pull-request discussions,
//! comments, pull-request metadata, and reviews. `normalize` converts provider DTOs into checked
//! `forgesync-core::content` values. [`RestThreadPage`], [`RestCommentPage`], and
//! [`RestReviewPage`] return one page plus continuation context; [`ThreadListState`] selects the
//! enumeration scope.
//!
//! The engine calls these functions while recording durable cursors and per-family outcomes. A
//! page can be valid even when a later page fails. Do not turn a page result into complete
//! collection membership until the engine and store have verified the whole selected page set.
//!
//! Private `wire` response structs retain provider spelling and nullable fields until
//! normalization. Archive rows, CLI JSON shapes, and domain observations belong to other crates.
//! The REST functions do not write to SQLite or choose a credential.

use forgesync_core::content::{Comment, Discussion, Review};
use reqwest::Url;

/// One normalized issue-list page and its validated next-page destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestThreadPage {
    /// Issues and pull requests returned by the repository issues endpoint.
    pub discussions: Vec<Discussion>,
    /// Next page URL from the provider, when one remains.
    pub next_page: Option<Url>,
}

/// One normalized issue-comment page and its validated next-page destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestCommentPage {
    /// Discussion comments returned by the issues comments endpoint.
    pub comments: Vec<Comment>,
    /// Next page URL from the provider, when one remains.
    pub next_page: Option<Url>,
}

/// One normalized pull-request review page and its validated next-page destination.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestReviewPage {
    /// Submitted and pending reviews returned by the pull-request reviews endpoint.
    pub reviews: Vec<Review>,
    /// Next page URL from the provider, when one remains.
    pub next_page: Option<Url>,
}

/// Source states selected from the GitHub issues endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadListState {
    /// Include issues and pull requests in either source state.
    All,
    /// Include only currently open issues and pull requests.
    Open,
    /// Include only currently closed issues and pull requests.
    Closed,
}

mod fetch;
mod normalize;
mod wire;

pub use fetch::{
    fetch_issue_comment_page, fetch_pull_request_metadata, fetch_pull_request_review_page,
    fetch_repository, fetch_thread_page, fetch_thread_page_in_scope, issue_comment_list_url,
    thread_list_url, thread_list_url_in_scope,
};

#[cfg(test)]
mod tests;
