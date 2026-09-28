//! Read-only GitHub API access and typed provider failures.
//!
//! This crate accepts credentials from its caller. Credential discovery and process environment
//! access belong to the application boundary.

mod error;
mod resources;
mod token;
mod transport;

pub use error::{ApiFailureKind, GitHubError};
pub use resources::{RestThreadPage, fetch_repository, fetch_thread_page, thread_list_url};
pub use token::GitHubToken;
pub use transport::{GitHubClient, GitHubClientConfig, GitHubResponse, RetryPolicy};
