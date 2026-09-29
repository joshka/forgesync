//! # Bounded embedding worker lifetime
//!
//! `BatchScheduler` owns pending requests and the workers serving them. It fills available slots,
//! waits for one outcome, and delegates successful responses to `EmbeddingWriter` before scheduling
//! more. Individual provider errors become report entries while other requests continue.
//!
//! Cancellation stops scheduling, aborts active workers, and marks the report interrupted. A worker
//! panic or persistence failure is fatal. Both cancellation and fatal failure drain every worker
//! before returning to the writer, which then releases the fence. Dropping a `JoinSet` alone aborts
//! its tasks but does not wait for their cleanup; normal execution therefore drains explicitly.
//!
//! Batch construction and service requests live in `batches`. Source compatibility lives in
//! `selection` and `chunks`; no scheduler decision changes cache or chunk identity.

use std::collections::VecDeque;
use std::ops::ControlFlow;

use forgesync_core::embedding::EmbeddingVector;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::embedding_client::{EmbeddingClient, EmbeddingClientError};
use crate::embeddings::batches::{EmbeddingBatch, make_batches};
use crate::embeddings::execution::EmbeddingWriter;
use crate::embeddings::selection::EmbeddingTask;
use crate::embeddings::{EmbeddingBatchFailure, EmbeddingReport};
use crate::error::EngineError;

/// One request's retained input and provider result, before writer validation.
type BatchResponse = (
    EmbeddingBatch,
    Result<Vec<EmbeddingVector>, EmbeddingClientError>,
);

/// Pending requests and active tasks for one configured service and cancellation scope.
pub struct BatchScheduler {
    /// Requests not yet started, in deterministic selection order.
    pending: VecDeque<EmbeddingBatch>,
    /// Active service requests, bounded by client concurrency.
    workers: JoinSet<BatchResponse>,
    /// Cloneable request capability and validated service limits.
    client: EmbeddingClient,
    /// Caller cancellation shared with every request worker.
    cancellation: CancellationToken,
}

impl BatchScheduler {
    /// Applies service count/byte budgets before starting any workers.
    pub fn new(
        tasks: Vec<EmbeddingTask>,
        client: &EmbeddingClient,
        cancellation: &CancellationToken,
    ) -> Self {
        let batches = make_batches(tasks, client.batch_size(), client.max_batch_input_bytes());
        Self {
            pending: VecDeque::from(batches),
            workers: JoinSet::new(),
            client: client.clone(),
            cancellation: cancellation.clone(),
        }
    }

    /// Processes outcomes and drains all tasks before the writer can release its lease.
    pub async fn run(
        mut self,
        writer: &EmbeddingWriter<'_>,
        report: &mut EmbeddingReport,
    ) -> Result<(), EngineError> {
        let result = self.process(writer, report).await;
        self.finish(result).await
    }

    /// Aborts on fatal failure and waits for every worker to drop its request resources.
    async fn finish(&mut self, result: Result<(), EngineError>) -> Result<(), EngineError> {
        if result.is_err() {
            self.workers.abort_all();
        }
        while self.workers.join_next().await.is_some() {}
        result
    }

    /// Alternates slot filling and one completed response until exhausted or cancelled.
    async fn process(
        &mut self,
        writer: &EmbeddingWriter<'_>,
        report: &mut EmbeddingReport,
    ) -> Result<(), EngineError> {
        loop {
            self.fill_slots();
            if self.workers.is_empty() {
                return Ok(());
            }
            let Some(response) = self.next(report).await? else {
                return Ok(());
            };
            if self.apply(writer, report, response).await?.is_break() {
                return Ok(());
            }
        }
    }

    /// Starts pending requests up to the configured concurrency limit.
    fn fill_slots(&mut self) {
        while self.workers.len() < self.client.concurrency() {
            let Some(batch) = self.pending.pop_front() else {
                break;
            };
            self.workers
                .spawn(batch.request(self.client.clone(), self.cancellation.clone()));
        }
    }

    /// Waits for one worker or marks cancellation and aborts the active set.
    async fn next(
        &mut self,
        report: &mut EmbeddingReport,
    ) -> Result<Option<BatchResponse>, EngineError> {
        tokio::select! {
            _ = self.cancellation.cancelled() => {
                let _ = self.cancel(report);
                Ok(None)
            }
            response = self.workers.join_next() => {
                response.transpose().map_err(|_| EngineError::EmbeddingWorkerFailed)
            }
        }
    }

    /// Dispatches provider outcomes while distinguishing caller cancellation from batch failure.
    async fn apply(
        &mut self,
        writer: &EmbeddingWriter<'_>,
        report: &mut EmbeddingReport,
        response: BatchResponse,
    ) -> Result<ControlFlow<()>, EngineError> {
        let (batch, result) = response;
        match result {
            Ok(vectors) => Self::persist(writer, report, batch, vectors).await,
            Err(EmbeddingClientError::Cancelled) if self.cancellation.is_cancelled() => {
                Ok(self.cancel(report))
            }
            Err(error) => Ok(Self::record_failure(report, batch, error)),
        }
    }

    /// Counts a completed batch only after its vectors have all persisted successfully.
    async fn persist(
        writer: &EmbeddingWriter<'_>,
        report: &mut EmbeddingReport,
        batch: EmbeddingBatch,
        vectors: Vec<EmbeddingVector>,
    ) -> Result<ControlFlow<()>, EngineError> {
        let count = vectors.len();
        writer.persist(batch, vectors).await?;
        report.chunks_embedded = report.chunks_embedded.saturating_add(count);
        Ok(ControlFlow::Continue(()))
    }

    /// Keeps a sanitized failed-request summary and allows independent batches to continue.
    fn record_failure(
        report: &mut EmbeddingReport,
        batch: EmbeddingBatch,
        error: EmbeddingClientError,
    ) -> ControlFlow<()> {
        report.failed_batches.push(EmbeddingBatchFailure {
            chunks: batch.tasks.len(),
            code: error.code(),
            message: error.to_string(),
        });
        ControlFlow::Continue(())
    }

    /// Marks caller interruption and prevents active requests from outliving execution.
    fn cancel(&mut self, report: &mut EmbeddingReport) -> ControlFlow<()> {
        report.cancelled = true;
        self.workers.abort_all();
        ControlFlow::Break(())
    }
}

#[cfg(test)]
#[path = "scheduling_tests.rs"]
mod tests;
