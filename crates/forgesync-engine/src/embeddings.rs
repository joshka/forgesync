//! # Materialize and persist document vectors
//!
//! `embed_documents` selects recipe-compatible documents, sends bounded batches through
//! `EmbeddingClient`, and stores the returned chunks. `EmbeddingReport` counts completed work and
//! `EmbeddingBatchFailure` retains partial failure details.
//!
//! Embeddings are derived from a document and service identity. A source observation alone does
//! not make an old vector current. Search checks compatibility before using stored vectors, while
//! this workflow produces the compatible material when requested.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_core::embedding::EmbeddingVector;
use forgesync_core::identity::ThreadId;
use forgesync_store::archive::Archive;
use forgesync_store::embeddings::{EmbeddingChunkInput, StoredEmbeddingChunk};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

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

struct DocumentChunk {
    index: u32,
    count: u32,
    hash: String,
    text: String,
}

struct EmbeddingTask {
    document: Arc<Document>,
    chunk: DocumentChunk,
}

struct EmbeddingBatch {
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

/// Selects chunks matching the current document and model identity.
fn compatible_chunks(
    existing: Vec<StoredEmbeddingChunk>,
    chunks: Vec<DocumentChunk>,
    count: u32,
    expected_dimensions: Option<u32>,
) -> CompatibleChunks {
    let current = existing
        .into_iter()
        .filter(|stored| {
            stored.count == count
                && expected_dimensions.is_none_or(|expected| stored.vector.dimensions() == expected)
        })
        .map(|stored| (stored.index, stored.chunk_hash))
        .collect::<std::collections::HashMap<_, _>>();
    let mut pending = Vec::new();
    let mut skipped = 0usize;
    for chunk in chunks {
        let index = chunk.index;
        let count = chunk.count;
        let hash = chunk.hash.clone();
        if current.get(&index).is_some_and(|stored| stored == &hash) {
            skipped = skipped.saturating_add(1);
        } else {
            pending.push(DocumentChunk {
                index,
                count,
                hash,
                text: chunk.text,
            });
        }
    }
    CompatibleChunks { pending, skipped }
}

struct CompatibleChunks {
    pending: Vec<DocumentChunk>,
    skipped: usize,
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

/// Splits one document into deterministic model inputs.
fn chunk_document(text: &str, max_bytes: usize) -> Result<Vec<DocumentChunk>, EngineError> {
    if max_bytes < 4 {
        return Err(EngineError::EmbeddingWorkerFailed);
    }
    let mut remaining = text.trim();
    let mut chunks = Vec::new();
    while !remaining.is_empty() {
        if remaining.len() <= max_bytes {
            chunks.push(remaining.to_owned());
            break;
        }
        let mut boundary = 0usize;
        for (index, character) in remaining.char_indices() {
            let next = index + character.len_utf8();
            if next > max_bytes {
                break;
            }
            boundary = next;
        }
        if boundary == 0 {
            return Err(EngineError::InvalidEmbeddingInput);
        }
        let split = remaining[..boundary]
            .char_indices()
            .rev()
            .find(|(_, character)| character.is_whitespace())
            .map(|(index, _)| index)
            .filter(|index| *index > 0)
            .unwrap_or(boundary);
        chunks.push(remaining[..split].trim_end().to_owned());
        remaining = remaining[split..].trim_start();
    }
    chunks.retain(|chunk| !chunk.is_empty());
    let count = u32::try_from(chunks.len()).map_err(|_| EngineError::InvalidEmbeddingInput)?;
    Ok(chunks
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let index = u32::try_from(index).expect("chunk count fits u32");
            DocumentChunk {
                index,
                count,
                hash: chunk_hash(index, &text),
                text,
            }
        })
        .collect())
}

/// Hashes one chunk with its recipe context for reuse decisions.
fn chunk_hash(index: u32, text: &str) -> String {
    let mut hasher = Sha256::new();
    add_hash_field(&mut hasher, b"forgesync-embedding-chunk-v1");
    add_hash_field(&mut hasher, &index.to_be_bytes());
    add_hash_field(&mut hasher, text.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Adds a length-delimited field to the stable chunk hash.
fn add_hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(value);
}

#[cfg(test)]
mod tests {
    use super::{chunk_document, make_batches};

    #[test]
    fn chunks_are_deterministic_utf8_safe_and_within_the_byte_budget() {
        let text = "first phrase 🦀 and another very long phrase";
        let first = chunk_document(text, 16).expect("chunks");
        let second = chunk_document(text, 16).expect("repeat chunks");

        assert_eq!(first.len(), second.len());
        assert!(first.iter().all(|chunk| chunk.text.len() <= 16));
        assert!(first.iter().all(|chunk| chunk.hash.len() == 64));
        assert_eq!(
            first
                .iter()
                .map(|chunk| chunk.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            text
        );
        assert!(first.iter().zip(second).all(|(left, right)| {
            left.index == right.index && left.hash == right.hash && left.text == right.text
        }));
    }

    #[test]
    fn chunk_hash_changes_when_the_chunk_position_changes() {
        let first = chunk_document("same", 16).expect("first");
        let later = chunk_document("prefix same", 6).expect("later");

        assert_ne!(first[0].hash, later[1].hash);
    }

    #[test]
    fn request_batches_obey_count_and_combined_byte_limits() {
        let tasks = (0..5)
            .map(|index| super::EmbeddingTask {
                document: std::sync::Arc::new(test_document()),
                chunk: super::DocumentChunk {
                    index,
                    count: 5,
                    hash: format!("{index:064x}"),
                    text: "four".to_owned(),
                },
            })
            .collect();
        let batches = make_batches(tasks, 2, 8);

        assert_eq!(
            batches
                .iter()
                .map(|batch| batch.tasks.len())
                .collect::<Vec<_>>(),
            [2, 2, 1]
        );
        assert!(batches.iter().all(|batch| {
            batch
                .tasks
                .iter()
                .map(|task| task.chunk.text.len())
                .sum::<usize>()
                <= 8
        }));
    }

    fn test_document() -> forgesync_core::document::Document {
        use forgesync_core::document::{Document, DocumentRecipe};
        use forgesync_core::identity::{
            GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber,
        };
        use forgesync_core::timestamp::UtcTimestamp;

        let repository = RepositoryId::new(
            GitHubHost::parse("github.com").expect("host"),
            ProviderId::new("1").expect("repository ID"),
        );
        Document::new(
            ThreadId::new(
                repository,
                ProviderId::new("2").expect("thread ID"),
                ThreadNumber::new(3).expect("thread number"),
            ),
            DocumentRecipe::OriginalBody,
            "Title".to_owned(),
            "body".to_owned(),
            "body".to_owned(),
            UtcTimestamp::parse("2026-09-01T00:00:00Z").expect("timestamp"),
        )
    }
}
