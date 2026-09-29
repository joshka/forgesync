//! Read-only GitHub API access and typed provider failures.
//!
//! This crate accepts credentials from its caller. Credential discovery and process environment
//! access belong to the application boundary.
//! [`transport`] owns HTTP clients and retries. [`resources`] and [`review_threads`] acquire and
//! normalize the supported REST and GraphQL resource families. [`error`] classifies provider
//! failures, and [`token`] holds a credential without exposing it in debug output.

pub mod error;
pub mod resources;
pub mod review_threads;
pub mod token;
pub mod transport;
