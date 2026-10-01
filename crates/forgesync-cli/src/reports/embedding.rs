//! Embedding command output.
//!
//! An embedding failure does not invalidate archived GitHub observations.

use forgesync_core::document::DocumentRecipe;
use forgesync_engine::embeddings::EmbeddingReport;
use forgesync_engine::refresh::{RefreshDocumentFailure, RefreshStageFailure, RefreshStageStatus};
use serde::Serialize;

use crate::error::Exit;
use crate::reports::sync::refresh_status_name;

/// The engine's embedding stage result with the repository scope and service identity attached.
#[derive(Debug, Serialize)]
pub struct EmbeddingOutput {
    pub repositories: Vec<String>,
    pub recipe: DocumentRecipe,
    pub endpoint: String,
    pub model: String,
    /// Absent when the service chooses its native dimensions.
    pub dimensions: Option<u32>,
    pub status: RefreshStageStatus,
    pub report: EmbeddingReport,
    pub documents_materialized: usize,
    pub document_failures: Vec<RefreshDocumentFailure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<RefreshStageFailure>,
}

impl EmbeddingOutput {
    pub fn exit_status(&self) -> Exit {
        match self.status {
            RefreshStageStatus::Complete => Exit::Success,
            RefreshStageStatus::Partial | RefreshStageStatus::Deferred => Exit::Partial,
            RefreshStageStatus::Interrupted => Exit::Interrupted,
            RefreshStageStatus::Failed => Exit::Failure,
        }
    }
}

impl EmbeddingOutput {
    pub fn summary(&self) -> String {
        let summary = format!(
            "Embedding {}: {} documents, {} chunks embedded, {} already current, {} failed batches, {} document failures using {} ({})",
            refresh_status_name(self.status),
            self.report.documents,
            self.report.chunks_embedded,
            self.report.chunks_skipped,
            self.report.failed_batches.len(),
            self.document_failures.len(),
            self.model,
            self.endpoint
        );
        match self.representative_failure() {
            Some(failure) => format!("{summary}; {failure}"),
            None => summary,
        }
    }

    /// Stage failure first, then the first failed batch, then the first failed document; the full
    /// collections remain in JSON.
    fn representative_failure(&self) -> Option<&str> {
        self.failure
            .as_ref()
            .map(|failure| failure.message.as_str())
            .or_else(|| {
                self.report
                    .failed_batches
                    .first()
                    .map(|failure| failure.message.as_str())
            })
            .or_else(|| {
                self.document_failures
                    .first()
                    .map(|failure| failure.message.as_str())
            })
    }
}

#[cfg(test)]
#[path = "embedding_tests.rs"]
mod tests;
