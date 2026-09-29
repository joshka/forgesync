//! # Human summaries and report adapters
//!
//! Report types collect the outcome data command handlers need to present. Child modules format
//! archive status, thread views, sync and refresh stages, embeddings, clusters, and run history.
//!
//! Presentation belongs here rather than in the engine. Commands can select human or JSON output
//! without changing workflow policy, and readers can find terminal wording without following
//! network or database operations.

use forgesync_core::document::DocumentRecipe;
use forgesync_engine::embeddings::EmbeddingReport;
use forgesync_engine::refresh::{RefreshDocumentFailure, RefreshStageFailure, RefreshStageStatus};
use forgesync_store::archive::ArchiveInfo;
use forgesync_store::migration::MigrationReport;
use serde::Serialize;

mod archive;
mod clusters;
mod embedding;
mod runs;
mod sync;
mod threads;

pub use archive::{archive_status_summary, archive_summary, doctor_summary, migration_summary};
pub use clusters::{
    cluster_build_summary, cluster_decision_summary, cluster_detail_summary, cluster_page_summary,
};
pub use embedding::embedding_summary;
pub use runs::{retry_summary, run_detail_summary, run_list_summary};
pub use sync::{outcome_exit_code, refresh_status_name, refresh_summary, sync_summary};
pub use threads::{family_name, render_search_page, render_thread_detail, render_thread_page};

#[derive(Serialize)]
pub struct SyncFailure {
    pub code: &'static str,
    pub message: String,
}

#[derive(Serialize)]
pub struct EmbeddingOutput {
    pub repositories: Vec<String>,
    pub recipe: DocumentRecipe,
    pub endpoint: String,
    pub model: String,
    pub dimensions: Option<u32>,
    pub status: RefreshStageStatus,
    pub report: EmbeddingReport,
    pub documents_materialized: usize,
    pub document_failures: Vec<RefreshDocumentFailure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<RefreshStageFailure>,
}

#[derive(Serialize)]
pub struct ClusterDecisionOutput {
    pub cluster_id: u64,
    pub action: &'static str,
}

#[derive(Serialize)]
pub struct MigrationOutput {
    pub migration: MigrationReport,
    pub archive: ArchiveInfo,
}
