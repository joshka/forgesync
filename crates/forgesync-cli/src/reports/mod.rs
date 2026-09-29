//! Human-readable command reports and JSON output shapes.

use super::*;

mod archive;
mod clusters;
mod embedding;
mod runs;
mod sync;
mod threads;

pub(super) use archive::*;
pub(super) use clusters::*;
pub(super) use embedding::*;
pub(super) use runs::*;
pub(super) use sync::*;
pub(super) use threads::*;

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
