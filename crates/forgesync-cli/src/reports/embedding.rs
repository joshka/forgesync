//! # Explain embedding materialization results
//!
//! Embedding output summarizes completed documents or chunks and failed batches. It translates a
//! structured engine report into a short terminal account of derived-data work.
//!
//! A failure here does not invalidate archived GitHub observations. The summary should make
//! partial completion and the next actionable scope visible without exposing service payloads.
//!
//! [`EmbeddingOutput`] attaches repository scope and configured service identity to the engine's
//! stage result for command JSON and human output. The summary shows one representative failure,
//! preferring the stage failure, then the first batch failure, then a document failure. Full
//! failure collections remain in the structured output rather than being replaced by that short
//! message.

use forgesync_core::document::DocumentRecipe;
use forgesync_engine::embeddings::EmbeddingReport;
use forgesync_engine::refresh::{RefreshDocumentFailure, RefreshStageFailure, RefreshStageStatus};
use serde::Serialize;

use crate::error::Exit;
use crate::reports::sync::refresh_status_name;

/// Command projection of embedding service identity, materialization, and batch outcomes.
///
/// Engine reports describe derived work; this DTO attaches the selected repository scope and
/// configured service details for process output. Partial document or batch failures remain
/// inspectable without changing archived GitHub observations. The summary and JSON serialization
/// consume the same value after the command has closed its writable archive.
#[derive(Debug, Serialize)]
pub struct EmbeddingOutput {
    /// Host-qualified repository URLs in the command's deduplicated execution order.
    pub repositories: Vec<String>,
    /// Document recipe used when materializing text for compatible vectors.
    pub recipe: DocumentRecipe,
    /// Normalized embedding service endpoint identity reported by the client.
    pub endpoint: String,
    /// Configured embedding model identity used for this operation.
    pub model: String,
    /// Requested output dimensions; absent when the service chooses its native dimensions.
    pub dimensions: Option<u32>,
    /// Complete, partial, deferred, interrupted, or failed state of this selected stage.
    pub status: RefreshStageStatus,
    /// Document/chunk totals and isolated batch failures from engine execution.
    pub report: EmbeddingReport,
    /// Number of discussions whose derived documents were successfully materialized.
    pub documents_materialized: usize,
    /// Isolated document materialization failures that did not hide other discussions' results.
    pub document_failures: Vec<RefreshDocumentFailure>,
    /// Stage-wide safe failure when present; omitted from JSON when absent.
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
    /// Explains completed derived work and one representative failure in human output.
    ///
    /// Full failure collections remain in JSON. This brief account does not replace them or imply
    /// that a document/batch failure invalidates archived provider observations.
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

    /// Selects stage failure first, then the first failed batch, then the first failed document.
    ///
    /// Priority follows the scope of the failure; it is independent of diagnostic message wording.
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
