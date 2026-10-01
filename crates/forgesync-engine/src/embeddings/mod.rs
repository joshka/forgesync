//! Materialize and persist document vectors.
//!
//! A source observation alone does not make an old vector current: compatibility is the document
//! identity plus the service endpoint/model. Selection reads reusable vectors before the writer
//! lease is claimed; only pending work acquires it.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use forgesync_core::document::Document;
use forgesync_core::embedding::EmbeddingVector;
use forgesync_store::archive::Archive;
use forgesync_store::embeddings::EmbeddingChunkInput;
use forgesync_store::leases::ArchiveLeaseToken;
use serde::Serialize;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

mod chunks;

use chunks::{DocumentChunk, chunk_document, compatible_chunks};

use crate::clock::now_utc;
use crate::embedding_client::{EmbeddingClient, EmbeddingClientError};
use crate::error::EngineError;
use crate::lease::with_writer_lease;

/// Decides whether compatible persisted vectors can satisfy this embedding request.
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
    /// Number of distinct source documents considered.
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

impl EmbeddingReport {
    /// Accumulates another report, keeping every failed batch and any cancellation.
    pub fn add(&mut self, other: Self) {
        self.documents += other.documents;
        self.chunks_selected += other.chunks_selected;
        self.chunks_embedded += other.chunks_embedded;
        self.chunks_skipped += other.chunks_skipped;
        self.failed_batches.extend(other.failed_batches);
        self.cancelled |= other.cancelled;
    }
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
    let (mut report, tasks) = select_tasks(archive, client, documents, policy).await?;
    if tasks.is_empty() {
        return Ok(report);
    }
    with_writer_lease(
        archive,
        lease_duration(client),
        cancellation,
        async |lease, cancellation| {
            execute(archive, lease, client, tasks, cancellation, &mut report).await
        },
    )
    .await?;
    Ok(report)
}

/// Embeds documents under a writer lease the caller already holds.
pub(crate) async fn embed_documents_fenced(
    archive: &Archive,
    lease: &ArchiveLeaseToken,
    client: &EmbeddingClient,
    documents: &[Document],
    policy: EmbeddingPolicy,
    cancellation: &CancellationToken,
) -> Result<EmbeddingReport, EngineError> {
    let (mut report, tasks) = select_tasks(archive, client, documents, policy).await?;
    execute(archive, lease, client, tasks, cancellation, &mut report).await?;
    Ok(report)
}

/// Lease lifetime covering one batch's request budget plus a persistence margin.
pub(crate) fn lease_duration(client: &EmbeddingClient) -> Duration {
    client
        .request_budget()
        .saturating_add(Duration::from_secs(60))
}

/// Pending model input tied to the full source document used for fenced persistence.
struct EmbeddingTask {
    document: Arc<Document>,
    chunk: DocumentChunk,
}

/// Splits each distinct document version and separates reusable chunks from pending requests.
async fn select_tasks(
    archive: &Archive,
    client: &EmbeddingClient,
    documents: &[Document],
    policy: EmbeddingPolicy,
) -> Result<(EmbeddingReport, Vec<EmbeddingTask>), EngineError> {
    let mut report = EmbeddingReport::default();
    let mut tasks = Vec::new();
    let mut seen = HashSet::new();
    for document in documents {
        let identity = (
            &document.source_identity,
            document.recipe,
            &document.content_hash,
        );
        if !seen.insert(identity) {
            continue;
        }
        report.documents += 1;
        let chunks = chunk_document(&document.text, client.max_input_bytes())?;
        report.chunks_selected += chunks.len();
        let Some(count) = chunks.first().map(|chunk| chunk.count) else {
            continue;
        };
        let existing = archive
            .embedding_chunks(document, client.endpoint_identity(), client.model(), count)
            .await?;
        let pending = match policy {
            EmbeddingPolicy::Replace => chunks,
            EmbeddingPolicy::Missing => {
                let compatible = compatible_chunks(existing, chunks, client.dimensions());
                report.chunks_skipped += compatible.skipped;
                compatible.pending
            }
        };
        let document = Arc::new(document.clone());
        tasks.extend(pending.into_iter().map(|chunk| EmbeddingTask {
            document: Arc::clone(&document),
            chunk,
        }));
    }
    Ok((report, tasks))
}

type BatchResponse = (
    Vec<EmbeddingTask>,
    Result<Vec<EmbeddingVector>, EmbeddingClientError>,
);

/// Requests every batch (the client bounds concurrency) and persists responses as they arrive.
///
/// Individual provider errors become report entries. Cancellation marks the report interrupted.
/// Workers are always aborted and drained before returning, so no request outlives the lease.
async fn execute(
    archive: &Archive,
    lease: &ArchiveLeaseToken,
    client: &EmbeddingClient,
    tasks: Vec<EmbeddingTask>,
    cancellation: &CancellationToken,
    report: &mut EmbeddingReport,
) -> Result<(), EngineError> {
    let mut workers = JoinSet::new();
    for batch in make_batches(tasks, client.batch_size(), client.max_batch_input_bytes()) {
        let client = client.clone();
        let cancellation = cancellation.clone();
        workers.spawn(async move {
            let input = batch
                .iter()
                .map(|task| task.chunk.text.clone())
                .collect::<Vec<_>>();
            let result = client.embed(&input, &cancellation).await;
            (batch, result)
        });
    }
    let result = collect(archive, lease, client, &mut workers, cancellation, report).await;
    drain(&mut workers).await;
    result
}

async fn collect(
    archive: &Archive,
    lease: &ArchiveLeaseToken,
    client: &EmbeddingClient,
    workers: &mut JoinSet<BatchResponse>,
    cancellation: &CancellationToken,
    report: &mut EmbeddingReport,
) -> Result<(), EngineError> {
    loop {
        let joined = tokio::select! {
            _ = cancellation.cancelled() => None,
            joined = workers.join_next() => match joined {
                Some(joined) => Some(joined.map_err(|_| EngineError::EmbeddingWorkerFailed)?),
                None => return Ok(()),
            },
        };
        let Some((batch, result)) = joined else {
            report.cancelled = true;
            return Ok(());
        };
        match result {
            Ok(vectors) => {
                let count = vectors.len();
                persist(archive, lease, client, batch, vectors).await?;
                report.chunks_embedded += count;
            }
            Err(EmbeddingClientError::Cancelled) if cancellation.is_cancelled() => {
                report.cancelled = true;
                return Ok(());
            }
            Err(error) => report.failed_batches.push(EmbeddingBatchFailure {
                chunks: batch.len(),
                code: error.code(),
                message: error.to_string(),
            }),
        }
    }
}

/// Aborts outstanding requests and waits for each to drop its resources.
async fn drain(workers: &mut JoinSet<BatchResponse>) {
    workers.abort_all();
    while workers.join_next().await.is_some() {}
}

/// Stores a count-validated response in input order under the client's service identity.
async fn persist(
    archive: &Archive,
    lease: &ArchiveLeaseToken,
    client: &EmbeddingClient,
    batch: Vec<EmbeddingTask>,
    vectors: Vec<EmbeddingVector>,
) -> Result<(), EngineError> {
    if batch.len() != vectors.len() {
        return Err(EngineError::InvalidEmbeddingInput);
    }
    for (task, vector) in batch.into_iter().zip(vectors) {
        let chunk = EmbeddingChunkInput {
            endpoint: client.endpoint_identity(),
            model: client.model(),
            index: task.chunk.index,
            count: task.chunk.count,
            chunk_hash: &task.chunk.hash,
            vector: &vector,
        };
        archive
            .upsert_embedding_chunk_fenced(lease, &task.document, &chunk, now_utc()?)
            .await?;
    }
    Ok(())
}

/// Groups pending inputs under count and aggregate UTF-8 byte budgets, preserving order.
///
/// An input that alone exceeds the byte budget stays a singleton for the client to reject.
fn make_batches(
    tasks: Vec<EmbeddingTask>,
    max_inputs: usize,
    max_bytes: usize,
) -> Vec<Vec<EmbeddingTask>> {
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut current_bytes = 0usize;
    for task in tasks {
        let task_bytes = task.chunk.text.len();
        if !current.is_empty()
            && (current.len() >= max_inputs || current_bytes + task_bytes > max_bytes)
        {
            batches.push(std::mem::take(&mut current));
            current_bytes = 0;
        }
        current_bytes += task_bytes;
        current.push(task);
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

#[cfg(test)]
mod tests;
