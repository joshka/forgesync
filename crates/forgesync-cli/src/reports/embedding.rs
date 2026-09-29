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
    /// Returns the process result for the same structured stage state rendered in JSON and prose.
    /// Partial or deferred work is retryable (`3`); interruption keeps the shell convention
    /// (`130`).
    pub fn exit_status(&self) -> std::process::ExitCode {
        match self.status {
            RefreshStageStatus::Complete => std::process::ExitCode::SUCCESS,
            RefreshStageStatus::Partial | RefreshStageStatus::Deferred => {
                std::process::ExitCode::from(3)
            }
            RefreshStageStatus::Interrupted => std::process::ExitCode::from(130),
            RefreshStageStatus::Failed => std::process::ExitCode::FAILURE,
        }
    }
}

/// Explains the selected embedding work and partial failures in human output.
pub fn embedding_summary(output: &EmbeddingOutput) -> String {
    let failure = output
        .failure
        .as_ref()
        .map(|failure| failure.message.as_str())
        .or_else(|| {
            output
                .report
                .failed_batches
                .first()
                .map(|failure| failure.message.as_str())
        })
        .or_else(|| {
            output
                .document_failures
                .first()
                .map(|failure| failure.message.as_str())
        });
    let summary = format!(
        "Embedding {}: {} documents, {} chunks embedded, {} already current, {} failed batches, {} document failures using {} ({})",
        refresh_status_name(output.status),
        output.report.documents,
        output.report.chunks_embedded,
        output.report.chunks_skipped,
        output.report.failed_batches.len(),
        output.document_failures.len(),
        output.model,
        output.endpoint
    );
    failure.map_or(summary.clone(), |failure| format!("{summary}; {failure}"))
}

#[cfg(test)]
#[path = "embedding_tests.rs"]
mod tests;
