#![forbid(unsafe_code)]

//! # Forgesync: local-first GitHub discussions
//!
//! This package is the installable application and the module-oriented entry point for Rust
//! callers. Its modules re-export the existing domain, store, and engine owners without copying
//! their types or adding another workflow implementation. Direct dependencies on those libraries
//! remain useful when an integration needs their narrower or provider-specific APIs.
//!
//! # Read an existing archive
//!
//! [`archive`] owns explicit creation, migration, and opening. Opening does not create or refresh
//! a database. [`inspect`] reads archived discussions; the caller retains responsibility for the
//! archive lifetime and access mode.
//!
//! ```no_run
//! use forgesync::archive::Archive;
//! use forgesync::inspect::{ThreadListRequest, list_threads};
//!
//! # async fn read() -> Result<(), Box<dyn std::error::Error>> {
//! let archive = Archive::open_read_only("archive.sqlite").await?;
//! let request = ThreadListRequest::default();
//! let page = list_threads(&archive, &request).await?;
//! println!("{} discussions", page.items.len());
//! archive.close().await;
//! # Ok(())
//! # }
//! ```
//!
//! # Choose a workflow
//!
//! - [`sync`] acquires GitHub evidence; [`refresh`] composes acquisition and selected analysis.
//! - [`search`] supports local keyword reads and semantic queries. Semantic and hybrid queries send
//!   query text to an embedding service, using previously archived document vectors.
//! - [`documents`] and [`embeddings`] prepare retrieval evidence; [`clustering`] groups compatible
//!   archived vectors for local triage.
//! - [`runs`] inspects durable work and retries unresolved failures. [`mod@reference`] resolves
//!   repository and discussion selectors before workflows execute.
//! - [`identity`], [`content`], [`coverage`], [`observation`], and [`outcome`] describe the domain
//!   facts shared across these workflows. They remain the same types as in `forgesync-core`.
//!
//! Engine operations borrow an explicitly opened archive and take cancellation where required.
//! They return structured reports and typed [`error`] values, preserving committed partial work.
//! The facade itself reads no process environment, installs no diagnostics, and selects no path.
//!
//! # Product features and process ownership
//!
//! Default features `cli` and `tui` install the complete `forgesync` executable. The `cli` feature
//! enables the binary adapter to `forgesync-cli`, which owns parsing, configuration, credentials,
//! tracing, rendering, and exit codes. `tui` enables terminal browsing through that adapter and
//! implies `cli`. Neither frontend is re-exported as a domain module.
//!
//! Rust consumers can select `default-features = false` to use these APIs without frontend
//! dependencies. Installations without terminal browsing use `--no-default-features --features
//! cli`; ordinary installations need no feature flags. Public APIs remain under development.

pub use forgesync_core::{
    content, coverage, document, embedding, identity, observation, outcome, provider_data,
    timestamp,
};
pub use forgesync_engine::{
    clustering, documents, embedding_client, embeddings, enumeration, error, exact_search, inspect,
    reference, refresh, runs, search, sync,
};
pub use forgesync_store::archive;
