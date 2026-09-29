//! # Materialize and persist document vectors
//!
//! `embed_documents` selects recipe-compatible documents, sends bounded batches through
//! `EmbeddingClient`, and stores the returned chunks. `EmbeddingReport` counts completed work and
//! `EmbeddingBatchFailure` retains partial failure details.
//!
//! Embeddings are derived from a document and service identity. A source observation alone does
//! not make an old vector current. Search checks compatibility before using stored vectors, while
//! this workflow produces the compatible material when requested. `chunks` owns deterministic text
//! splitting and per-chunk reuse checks; scheduling and fenced persistence remain here.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_core::embedding::EmbeddingVector;
use forgesync_core::identity::ThreadId;
use forgesync_store::archive::Archive;
use forgesync_store::embeddings::EmbeddingChunkInput;
use serde::Serialize;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

mod chunks;

use chunks::{DocumentChunk, chunk_document, compatible_chunks};

use crate::documents::now_utc;
use crate::embedding_client::{EmbeddingClient, EmbeddingClientError};
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

/// Pending model input tied to the immutable source document used for fenced persistence.
struct EmbeddingTask {
    /// Shared full document identity and content hash, retained across its separate chunks.
    document: Arc<Document>,
    /// Deterministic position and text requested from the service.
    chunk: DocumentChunk,
}

/// Ordered pending inputs constrained by both service count and aggregate byte budgets.
struct EmbeddingBatch {
    /// Input order must match returned vector order; persistence rejects count mismatches.
    tasks: Vec<EmbeddingTask>,
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
    let mut report = EmbeddingReport {
        documents: 0,
        chunks_selected: 0,
        chunks_embedded: 0,
        chunks_skipped: 0,
        failed_batches: Vec::new(),
        cancelled: false,
    };
    let mut tasks = Vec::new();
    let mut seen = HashSet::<(ThreadId, DocumentRecipe, String)>::new();
    for document in documents {
        let identity = (
            document.source_identity.clone(),
            document.recipe,
            document.content_hash.clone(),
        );
        if !seen.insert(identity) {
            continue;
        }
        report.documents = report.documents.saturating_add(1);
        let chunks = chunk_document(&document.text, client.max_input_bytes())?;
        let count = u32::try_from(chunks.len()).map_err(|_| EngineError::InvalidEmbeddingInput)?;
        report.chunks_selected = report.chunks_selected.saturating_add(chunks.len());
        if count == 0 {
            continue;
        }
        let existing = archive
            .embedding_chunks(document, client.endpoint_identity(), client.model(), count)
            .await?;
        let document = Arc::new(document.clone());
        if policy == EmbeddingPolicy::Replace {
            tasks.extend(chunks.into_iter().map(|chunk| EmbeddingTask {
                document: Arc::clone(&document),
                chunk,
            }));
        } else {
            let compatible = compatible_chunks(existing, chunks, count, client.dimensions());
            tasks.extend(compatible.pending.into_iter().map(|chunk| EmbeddingTask {
                document: Arc::clone(&document),
                chunk,
            }));
            report.chunks_skipped = report.chunks_skipped.saturating_add(compatible.skipped);
        }
    }
    if tasks.is_empty() {
        return Ok(report);
    }

    let now = now_utc()?;
    let lease_duration = client
        .request_budget()
        .saturating_add(Duration::from_secs(60));
    let lease = archive.acquire_archive_lease(now, lease_duration).await?;
    let operation = process_batches(
        archive,
        client,
        tasks,
        &lease,
        lease_duration,
        cancellation,
        &mut report,
    )
    .await;
    let release_result = match now_utc() {
        Ok(released_at) => archive
            .release_archive_lease(&lease, released_at)
            .await
            .map(|_| ())
            .map_err(EngineError::from),
        Err(error) => Err(error),
    };
    match (operation, release_result) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(report),
    }
}

/// Runs bounded embedding batches while keeping successes when a later batch fails.
async fn process_batches(
    archive: &Archive,
    client: &EmbeddingClient,
    tasks: Vec<EmbeddingTask>,
    lease: &forgesync_store::leases::ArchiveLeaseToken,
    lease_duration: Duration,
    cancellation: &CancellationToken,
    report: &mut EmbeddingReport,
) -> Result<(), EngineError> {
    let mut pending = VecDeque::from(make_batches(
        tasks,
        client.batch_size(),
        client.max_batch_input_bytes(),
    ));
    let mut workers = JoinSet::new();
    loop {
        while workers.len() < client.concurrency() {
            let Some(batch) = pending.pop_front() else {
                break;
            };
            let client = client.clone();
            let cancellation = cancellation.clone();
            workers.spawn(async move {
                let input = batch
                    .tasks
                    .iter()
                    .map(|task| task.chunk.text.clone())
                    .collect::<Vec<_>>();
                let result = client.embed(&input, &cancellation).await;
                (batch, result)
            });
        }
        if workers.is_empty() {
            break;
        }
        let joined = tokio::select! {
            _ = cancellation.cancelled() => {
                report.cancelled = true;
                workers.abort_all();
                break;
            }
            result = workers.join_next() => result,
        };
        let Some(joined) = joined else {
            continue;
        };
        let (batch, result) = joined.map_err(|_| EngineError::EmbeddingWorkerFailed)?;
        match result {
            Ok(vectors) => {
                let persisted_chunks = vectors.len();
                persist_batch(archive, client, batch, vectors, lease, lease_duration).await?;
                report.chunks_embedded = report.chunks_embedded.saturating_add(persisted_chunks);
            }
            Err(EmbeddingClientError::Cancelled) if cancellation.is_cancelled() => {
                report.cancelled = true;
                workers.abort_all();
                break;
            }
            Err(error) => report.failed_batches.push(EmbeddingBatchFailure {
                chunks: batch.tasks.len(),
                code: error.code(),
                message: error.to_string(),
            }),
        }
    }
    while workers.join_next().await.is_some() {}
    Ok(())
}

/// Stores only validated vectors for a completed service batch.
async fn persist_batch(
    archive: &Archive,
    client: &EmbeddingClient,
    batch: EmbeddingBatch,
    vectors: Vec<EmbeddingVector>,
    lease: &forgesync_store::leases::ArchiveLeaseToken,
    lease_duration: Duration,
) -> Result<(), EngineError> {
    if batch.tasks.len() != vectors.len() {
        return Err(EngineError::InvalidEmbeddingInput);
    }
    for (task, vector) in batch.tasks.into_iter().zip(vectors) {
        let stored_at = now_utc()?;
        archive
            .heartbeat_archive_lease(lease, stored_at, lease_duration)
            .await?;
        let chunk = EmbeddingChunkInput {
            endpoint: client.endpoint_identity(),
            model: client.model(),
            index: task.chunk.index,
            count: task.chunk.count,
            chunk_hash: &task.chunk.hash,
            vector: &vector,
        };
        archive
            .upsert_embedding_chunk_fenced(lease, &task.document, &chunk, stored_at)
            .await?;
    }
    Ok(())
}

/// Groups pending inputs under both count and byte budgets.
fn make_batches(
    tasks: Vec<EmbeddingTask>,
    max_inputs: usize,
    max_bytes: usize,
) -> Vec<EmbeddingBatch> {
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut current_bytes = 0usize;
    for task in tasks {
        let task_bytes = task.chunk.text.len();
        let would_exceed = !current.is_empty()
            && (current.len() >= max_inputs
                || current_bytes.saturating_add(task_bytes) > max_bytes);
        if would_exceed {
            batches.push(EmbeddingBatch { tasks: current });
            current = Vec::new();
            current_bytes = 0;
        }
        current_bytes = current_bytes.saturating_add(task_bytes);
        current.push(task);
    }
    if !current.is_empty() {
        batches.push(EmbeddingBatch { tasks: current });
    }
    batches
}

#[cfg(test)]
mod tests;
