//! Read-only GitHub API access and typed provider failures.
//!
//! This crate accepts credentials from its caller. Credential discovery and process environment
//! access belong to the application boundary.

mod error;
mod resources;
mod token;
mod transport;

pub use error::{ApiFailureKind, GitHubError};
pub use resources::{
    RestThreadPage, ThreadListState, fetch_repository, fetch_thread_page,
    fetch_thread_page_in_scope, thread_list_url, thread_list_url_in_scope,
};
pub use token::GitHubToken;
pub use transport::{GitHubClient, GitHubClientConfig, GitHubResponse, RetryPolicy};
