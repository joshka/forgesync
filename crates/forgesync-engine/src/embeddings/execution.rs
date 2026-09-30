//! # Writer capability for embedding persistence
//!
//! `EmbeddingWriter` acquires the archive fence only after document selection has pending work.
//! It couples that fence to the exact client identity used by the batch scheduler, so stored
//! vectors retain the same endpoint/model identity as the requests producing them.
//!
//! `execute` waits for the scheduler to drain its workers before releasing the lease on success
//! or failure. A workflow error takes precedence over a subsequent release error. Successful
//! writes are retained for retries; the operation does not wrap provider I/O in a transaction.
//! Lease duration includes one complete request budget plus a persistence margin. Each chunk
//! write renews the fence before its upsert, using the same duration.
//!
//! Batch count validation precedes writes. Chunk writes can still fail after earlier chunks have
//! persisted, and the operation then returns the store error rather than a completed report.

use std::time::Duration;

use forgesync_core::embedding::EmbeddingVector;
use forgesync_store::archive::Archive;
use forgesync_store::embeddings::EmbeddingChunkInput;
use forgesync_store::leases::ArchiveLeaseToken;
use tokio_util::sync::CancellationToken;

use crate::clock::now_utc;
use crate::embedding_client::EmbeddingClient;
use crate::embeddings::EmbeddingReport;
use crate::embeddings::batches::EmbeddingBatch;
use crate::embeddings::scheduling::BatchScheduler;
use crate::embeddings::selection::EmbeddingTask;
use crate::error::EngineError;

/// Fence and service identity shared by every write in one embedding execution.
pub struct EmbeddingWriter<'a> {
    /// Archive receiving chunk writes and lease heartbeats.
    archive: &'a Archive,
    /// Service whose responses are being persisted, including endpoint and model identity.
    client: &'a EmbeddingClient,
    /// Writer fence checked by every persisted chunk.
    lease: ArchiveLeaseToken,
    /// Full request budget plus margin, renewed before each write.
    lease_duration: Duration,
}

impl<'a> EmbeddingWriter<'a> {
    /// Acquires the writer fence before any selected service request is started.
    pub async fn acquire(
        archive: &'a Archive,
        client: &'a EmbeddingClient,
    ) -> Result<Self, EngineError> {
        let now = now_utc()?;
        let lease_duration = client
            .request_budget()
            .saturating_add(Duration::from_secs(60));
        let lease = archive.acquire_archive_lease(now, lease_duration).await?;
        Ok(Self {
            archive,
            client,
            lease,
            lease_duration,
        })
    }

    /// Runs selected requests, drains workers, and releases the fence before returning.
    /// The original execution error is preserved when release also fails.
    pub async fn execute(
        self,
        tasks: Vec<EmbeddingTask>,
        cancellation: &CancellationToken,
        report: &mut EmbeddingReport,
    ) -> Result<(), EngineError> {
        let scheduler = BatchScheduler::new(tasks, self.client, cancellation);
        let operation = scheduler.run(&self, report).await;
        let release = self.release().await;
        operation.and(release)
    }

    /// Persists a count-validated response in input order under this writer's service identity.
    pub async fn persist(
        &self,
        batch: EmbeddingBatch,
        vectors: Vec<EmbeddingVector>,
    ) -> Result<(), EngineError> {
        if batch.tasks.len() != vectors.len() {
            return Err(EngineError::InvalidEmbeddingInput);
        }
        for (task, vector) in batch.tasks.into_iter().zip(vectors) {
            self.persist_chunk(task, vector).await?;
        }
        Ok(())
    }

    /// Renews the lease and stores one vector with its full document and chunk identities.
    async fn persist_chunk(
        &self,
        task: EmbeddingTask,
        vector: EmbeddingVector,
    ) -> Result<(), EngineError> {
        let stored_at = now_utc()?;
        self.archive
            .heartbeat_archive_lease(&self.lease, stored_at, self.lease_duration)
            .await?;
        let chunk = EmbeddingChunkInput {
            endpoint: self.client.endpoint_identity(),
            model: self.client.model(),
            index: task.chunk.index,
            count: task.chunk.count,
            chunk_hash: &task.chunk.hash,
            vector: &vector,
        };
        self.archive
            .upsert_embedding_chunk_fenced(&self.lease, &task.document, &chunk, stored_at)
            .await?;
        Ok(())
    }

    /// Releases the active fence at the current source-independent wall clock.
    async fn release(&self) -> Result<(), EngineError> {
        let released_at = now_utc()?;
        self.archive
            .release_archive_lease(&self.lease, released_at)
            .await?;
        Ok(())
    }
}
