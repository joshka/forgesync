//! Human-readable command reports and JSON output shapes.

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

pub(super) use archive::{
    archive_status_summary, archive_summary, doctor_summary, migration_summary,
};
pub(super) use clusters::{
    cluster_build_summary, cluster_decision_summary, cluster_detail_summary, cluster_page_summary,
};
pub(super) use embedding::embedding_summary;
pub(super) use runs::{retry_summary, run_detail_summary, run_list_summary};
pub(super) use sync::{outcome_exit_code, refresh_status_name, refresh_summary, sync_summary};
pub(super) use threads::{
    family_name, render_search_page, render_thread_detail, render_thread_page,
};

#[derive(Serialize)]
pub(super) struct SyncFailure {
    pub(super) code: &'static str,
    pub(super) message: String,
}

#[derive(Serialize)]
pub(super) struct EmbeddingOutput {
    pub(super) repositories: Vec<String>,
    pub(super) recipe: DocumentRecipe,
    pub(super) endpoint: String,
    pub(super) model: String,
    pub(super) dimensions: Option<u32>,
    pub(super) status: RefreshStageStatus,
    pub(super) report: EmbeddingReport,
    pub(super) documents_materialized: usize,
    pub(super) document_failures: Vec<RefreshDocumentFailure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) failure: Option<RefreshStageFailure>,
}

#[derive(Serialize)]
pub(super) struct ClusterDecisionOutput {
    pub(super) cluster_id: u64,
    pub(super) action: &'static str,
}

#[derive(Serialize)]
pub(super) struct MigrationOutput {
    pub(super) migration: MigrationReport,
    pub(super) archive: ArchiveInfo,
}
