#![forbid(unsafe_code)]

//! # Local-first application workflows
//!
//! The engine coordinates provider acquisition, archive writes, offline inspection, search,
//! embeddings, and cluster analysis. It accepts an already opened `Archive`; the caller chooses
//! whether that handle is writable and supplies cancellation for long-running work. The engine
//! never owns CLI argument parsing or terminal rendering.
//!
//! `sync` and `enumeration` acquire source evidence through the GitHub adapter. `refresh` composes
//! acquisition with optional derived analysis. `documents`, `embeddings`, and `clustering` build
//! local search and triage material from archived discussions. `inspect`, `search`, and `runs`
//! expose offline reads and retry workflows. Provider DTOs are normalized before they reach the
//! store; SQL row details do not escape the store.
//!
//! Workflows return typed reports that preserve partial success. A failed discussion or resource
//! family should be visible without discarding successful work from the same run.
//!
//! # Start with an offline read
//!
//! Use [`inspect`] when the task is listing or inspecting what is already stored. The caller opens
//! and closes the archive; the engine borrows it for each operation. Filtering and pagination use
//! engine request types, while returned summaries describe normalized content and resource
//! coverage.
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
//! - [`refresh`] composes acquisition with selected analysis and retains each stage's partial
//!   result.
//! - [`documents`] materializes retrieval text; [`embeddings`] sends that text to a configured
//!   service and persists validated vectors. [`clustering`] groups compatible archived evidence.
//! - [`search`] keeps keyword queries offline. Semantic and hybrid queries use archived document
//!   vectors but send query text to the embedding service; they do not refresh source discussions.
//! - [`runs`] inspects durable work and plans retries from unresolved failures.
//!
//! Long-running workflows accept cancellation explicitly. Cancellation is a terminal workflow
//! outcome, not permission to discard already committed pages or bypass durable cleanup. Reports
//! and [`error::EngineError`] preserve that distinction for CLI and TUI callers.
//!
//! The library API remains under development. Persisted evidence and CLI contracts have dedicated
//! regression tests; incidental helper types and module paths may change as ownership becomes
//! clearer.

pub mod clustering;
pub mod documents;
pub mod embedding_client;
pub mod embeddings;
pub mod enumeration;
pub mod error;
pub mod exact_search;
pub mod inspect;
pub mod reference;
pub mod refresh;
pub mod runs;
pub mod search;
pub mod sync;
