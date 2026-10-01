//! Read-only GitHub acquisition and normalization for Forgesync workflows.
//!
//! The caller supplies the token and transport configuration; this crate never reads credentials,
//! opens an archive, or decides when to sync. A page result is not a claim that a resource family
//! is complete: the engine and store own that transition.
//!
//! # Acquire one normalized resource
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

pub mod error;
pub mod resources;
pub mod review_threads;
pub mod token;
pub mod transport;
