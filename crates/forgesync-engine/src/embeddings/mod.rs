//! # Materialize and persist document vectors
//!
//! `embed_documents` selects recipe-compatible documents, sends bounded batches through
//! `EmbeddingClient`, and stores the returned chunks. `EmbeddingReport` counts completed work and
//! `EmbeddingBatchFailure` retains partial failure details.
//!
//! Embeddings are derived from a document and service identity. A source observation alone does
//! not make an old vector current. Search checks compatibility before using stored vectors, while
//! this workflow produces the compatible material when requested. `selection` owns duplicate
//! detection, cache lookup, and selected/skipped accounting. `chunks` owns deterministic text
//! splitting and per-chunk reuse checks. `batches` bounds service requests; `scheduling` owns
//! worker cancellation and draining. `execution` owns writer fencing, persistence, and lease
//! release.

use forgesync_core::document::Document;
use forgesync_store::archive::Archive;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

mod batches;
mod chunks;
mod execution;
mod scheduling;
mod selection;

use execution::EmbeddingWriter;
use selection::EmbeddingSelection;

use crate::embedding_client::EmbeddingClient;
use crate::error::EngineError;

/// Decides whether compatible persisted vectors can satisfy this embedding request.
///
/// Use `Missing` for normal refresh and retry: successful earlier batches are reused. Use
/// `Replace` when intentionally rebuilding vectors, even if their document and service identities
/// still match. Replacement preserves the same writer fencing and partial-success rules.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EmbeddingPolicy {
    /// Reuse compatible chunks and request only missing or stale vectors.
    #[default]
    Missing,
    /// Request every selected chunk again.
    Replace,
}

impl EmbeddingPolicy {
    /// Converts a process-facing force flag at the application boundary.
    pub fn from_force(force: bool) -> Self {
        if force { Self::Replace } else { Self::Missing }
    }
}

/// Results of embedding the selected current documents with one configured service.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct EmbeddingReport {
    /// Number of source documents considered.
    pub documents: usize,
    /// Total current chunks after deterministic splitting.
    pub chunks_selected: usize,
    /// Chunks newly persisted by successful provider batches.
    pub chunks_embedded: usize,
    /// Chunks already covered by matching service/model/hash records.
    pub chunks_skipped: usize,
    /// Provider batches that failed after their bounded retry policy.
    pub failed_batches: Vec<EmbeddingBatchFailure>,
    /// True when cancellation stopped remaining batch work.
    pub cancelled: bool,
}

/// Sanitized failure summary for one batch; provider response bodies are never retained.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EmbeddingBatchFailure {
    /// Number of chunks in the failed request.
    pub chunks: usize,
    /// Stable machine-readable error classification.
    pub code: &'static str,
    /// Safe short message, which may include only a numeric HTTP status.
    pub message: String,
}

/// Embeds current documents, persisting each completed response batch under one writer fence.
///
/// Successful batches remain available if a later batch fails. Re-running the operation skips
/// compatible stored chunks and requests only missing or stale inputs.
pub async fn embed_documents(
    archive: &Archive,
    client: &EmbeddingClient,
    documents: &[Document],
    policy: EmbeddingPolicy,
    cancellation: &CancellationToken,
) -> Result<EmbeddingReport, EngineError> {
    let selection = EmbeddingSelection::collect(archive, client, documents, policy).await?;
    let mut report = selection.report;
    let tasks = selection.tasks;
    if tasks.is_empty() {
        return Ok(report);
    }

    let writer = EmbeddingWriter::acquire(archive, client).await?;
    writer.execute(tasks, cancellation, &mut report).await?;
    Ok(report)
}
