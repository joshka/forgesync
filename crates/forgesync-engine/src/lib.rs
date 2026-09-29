#![forbid(unsafe_code)]

//! Reusable local archive operations shared by frontends.

mod clustering;
mod documents;
mod embedding_client;
mod embeddings;
mod enumeration;
mod error;
mod exact_search;
mod inspect;
mod reference;
mod refresh;
mod runs;
mod search;
mod sync;

pub use clustering::{
    ClusterBuildReport, ClusterBuildRequest, ClusterListRequest, ClusterOptions, build_clusters,
    dismiss_cluster, exclude_cluster_member, include_cluster_member, list_clusters,
    restore_cluster, set_canonical_cluster_member, show_cluster,
};
pub use documents::{
    DocumentBuildReport, build_document, build_thread_document, materialize_thread_document,
};
pub use embedding_client::{EmbeddingClient, EmbeddingClientConfig, EmbeddingClientError};
pub use embeddings::{EmbeddingBatchFailure, EmbeddingReport, embed_documents};
pub use enumeration::{
    ThreadEnumerationReport, enumerate_repository_threads, enumerate_repository_threads_in_scope,
};
pub use error::EngineError;
pub use exact_search::cosine_similarity;
pub use forgesync_store::clusters::{
    ClusterDetail, ClusterLifecycle, ClusterMember, ClusterMemberRole, ClusterMemberState,
    ClusterPage, ClusterSummary,
};
pub use forgesync_store::reads::{
    ArchiveStatus, ThreadDetail, ThreadPage, ThreadSummary, ThreadTimelineEvent,
};
pub use forgesync_store::runs::{RunStatus, SyncJobStatus};
pub use inspect::{
    ThreadFilters, ThreadListRequest, ThreadSort, ThreadStateFilter, archive_status,
    list_repositories, list_threads, show_thread,
};
pub use reference::{ReferenceParseError, RepositorySelector, ThreadSelector};
pub use refresh::{
    EmbeddingServiceIdentity, RefreshAnalysisStage, RefreshClusterRepository,
    RefreshDocumentFailure, RefreshEmbeddingReport, RefreshReport, RefreshRequest, RefreshStage,
    RefreshStageFailure, RefreshStageKind, RefreshStageStatus, RefreshSyncOptions,
    embed_repositories, refresh,
};
pub use runs::{
    RetryPlan, RetryReport, RetryScope, list_runs, plan_run_retry, run_retry, show_run,
};
pub use search::{
    SearchHit, SearchMode, SearchProvenance, SearchRanking, SearchRequest, SearchResultPage,
    retrieve_threads, search_threads,
};
pub use sync::{
    SyncProgress, SyncProgressStatus, SyncReport, SyncRequest, SyncThreadScope, sync_repositories,
};
