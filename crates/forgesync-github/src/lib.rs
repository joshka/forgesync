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

pub mod error;
pub mod resources;
pub mod review_threads;
pub mod token;
pub mod transport;
