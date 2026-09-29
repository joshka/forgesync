//! Embedding command presentation.

use crate::reports::{EmbeddingOutput, refresh_status_name};

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
