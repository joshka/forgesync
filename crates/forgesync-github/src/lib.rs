//! Read-only GitHub API access and typed provider failures.
//!
//! This crate accepts credentials from its caller. Credential discovery and process environment
//! access belong to the application boundary.

mod error;
mod resources;
mod review_threads;
mod token;
mod transport;

pub use error::{ApiFailureKind, GitHubError};
pub use resources::{
    RestCommentPage, RestReviewPage, RestThreadPage, ThreadListState, fetch_issue_comment_page,
    fetch_pull_request_metadata, fetch_pull_request_review_page, fetch_repository,
    fetch_thread_page, fetch_thread_page_in_scope, issue_comment_list_url, thread_list_url,
    thread_list_url_in_scope,
};
pub use review_threads::{GraphqlCursor, GraphqlReviewThreadPage, fetch_review_thread_page};
pub use token::GitHubToken;
pub use transport::{GitHubClient, GitHubClientConfig, GitHubResponse, RetryPolicy};
