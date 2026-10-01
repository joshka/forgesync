#![forbid(unsafe_code)]

//! # Local-first application workflows
//!
//! The engine coordinates provider acquisition, archive writes, offline inspection, search,
//! embeddings, and cluster analysis. Every operation borrows an already opened `Archive`; the
//! caller chooses whether it is writable and supplies cancellation for long-running work.
//! Workflows return typed reports that preserve partial success: one failed discussion or family
//! stays visible without discarding successful work from the same run, and cancellation never
//! discards committed pages or bypasses durable cleanup.
//!
//! # Start with an offline read
//!
//! ```no_run
//! use forgesync_engine::inspect::{ThreadListRequest, list_threads};
//! use forgesync_store::archive::Archive;
//!
//! # async fn inspect() -> Result<(), Box<dyn std::error::Error>> {
//! let archive = Archive::open_read_only("archive.sqlite3").await?;
//! let request = ThreadListRequest::default();
//! let page = list_threads(&archive, &request).await?;
//! println!("{} discussions on this page", page.items.len());
//! archive.close().await;
//! # Ok(())
//! # }
//! ```
//!
//! # Choose acquisition or analysis deliberately
//!
//! - [`sync`] and [`enumeration`] contact GitHub and persist source evidence under writer fencing.
//! - [`refresh`] composes acquisition with selected analysis stages.
//! - [`documents`] materializes retrieval text; [`embeddings`] sends it to a configured service and
//!   persists vectors; [`clustering`] groups compatible archived evidence.
//! - [`search`] keeps keyword queries offline; semantic and hybrid queries send query text to the
//!   embedding service.
//! - [`runs`] inspects durable work and retries unresolved failures.

mod clock;
pub mod clustering;
pub mod documents;
pub mod embedding_client;
pub mod embeddings;
pub mod enumeration;
pub mod error;
pub mod exact_search;
pub mod inspect;
mod lease;
mod provider_failure;
mod query;
pub mod reference;
pub mod refresh;
pub mod runs;
mod scoring;
pub mod search;
pub mod sync;
