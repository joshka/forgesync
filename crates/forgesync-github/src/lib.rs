//! Read-only GitHub acquisition and normalization for Forgesync workflows.
//!
//! This crate accepts an explicitly supplied [`token::GitHubToken`] and transport configuration.
//! It does not discover credentials, open an archive, or decide when a repository should be
//! synced. The engine chooses a resource family and cancellation scope, calls this crate, then
//! hands checked core values to the store.
//!
//! The provider boundary has three related layers:
//!
//! - [`transport`] owns origin validation, bounded HTTP requests, retries, and page links.
//! - [`resources`] fetches REST repositories, discussions, comments, metadata, and reviews.
//! - [`review_threads`] handles GraphQL review threads and nested comment pagination.
//!
//! [`error`] preserves failure categories for retries and reports. Provider DTOs stay in the
//! relevant resource module; normalized domain values leave through `forgesync-core`. A page
//! result is not a claim that an entire resource family is complete. The engine and store own that
//! completeness transition, especially when later pages fail.
//!
//! # Acquire one normalized resource
//!
//! The application supplies credentials and the provider host. Client construction validates the
//! API origin; resource fetching uses that origin and returns core domain values. This example
//! makes a network request when run, so it is compiled as documentation but not executed as a
//! doctest.
//!
//! ```no_run
//! use forgesync_core::identity::GitHubHost;
//! use forgesync_github::resources::fetch_repository;
//! use forgesync_github::token::GitHubToken;
//! use forgesync_github::transport::{GitHubClient, GitHubClientConfig};
//! use tokio_util::sync::CancellationToken;
//!
//! # async fn acquire() -> Result<(), Box<dyn std::error::Error>> {
//! let host = GitHubHost::parse("github.com")?;
//! let config = GitHubClientConfig::new("https://api.github.com/".parse()?);
//! let token = GitHubToken::new("example-token")?;
//! let client = GitHubClient::new(config, Some(token))?;
//! let cancellation = CancellationToken::new();
//! let repository = fetch_repository(&client, &host, "ratatui", "ratatui", &cancellation).await?;
//! println!("{}", repository.full_name);
//! # Ok(())
//! # }
//! ```
//!
//! # Keep acquisition separate from application
//!
//! A normalized resource is not yet durable evidence. The engine associates it with acquisition
//! order, family scope, and completeness before calling the store. REST pages and GraphQL
//! connections must be traversed to a terminal result; a first successful page cannot establish
//! complete coverage. Provider failures retain categories for retry policy, while response bodies
//! and credentials stay out of error reports. Redirects and page links are checked against the
//! configured origin before any authorization header is sent.
//!
//! This is an application adapter with an evolving Rust API, rather than a general GitHub SDK. Add
//! resource operations for selected Forgesync workflows and keep write-back outside this boundary.

pub mod error;
pub mod resources;
pub mod review_threads;
pub mod token;
pub mod transport;
