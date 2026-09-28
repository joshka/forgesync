#![forbid(unsafe_code)]

//! Reusable local archive operations shared by frontends.

mod documents;
mod embedding_client;
mod embeddings;
mod enumeration;
mod error;
mod inspect;
mod reference;
mod runs;
mod search;
mod sync;

pub use documents::{
    DocumentBuildReport, build_document, build_thread_document, materialize_thread_document,
};
pub use embedding_client::{EmbeddingClient, EmbeddingClientConfig, EmbeddingClientError};
pub use embeddings::{EmbeddingBatchFailure, EmbeddingReport, embed_documents};
pub use enumeration::{
    ThreadEnumerationReport, enumerate_repository_threads, enumerate_repository_threads_in_scope,
};
pub use error::EngineError;
pub use forgesync_store::{ArchiveStatus, ThreadDetail, ThreadPage};
pub use inspect::{
    ThreadFilters, ThreadListRequest, ThreadSort, ThreadStateFilter, archive_status, list_threads,
    show_thread,
};
pub use reference::{ReferenceParseError, RepositorySelector, ThreadSelector};
pub use runs::{
    RetryPlan, RetryReport, RetryScope, list_runs, plan_run_retry, run_retry, show_run,
};
pub use search::{SearchMode, SearchRequest, search_threads};
pub use sync::{
    SyncProgress, SyncProgressStatus, SyncReport, SyncRequest, SyncThreadScope, sync_repositories,
};
