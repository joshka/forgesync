//! # Bounded model requests
//!
//! `make_batches` groups selected inputs under count and aggregate UTF-8 byte budgets.
//! `EmbeddingBatch` retains their order through the request and response, allowing the writer
//! to associate each vector with its source document and chunk.
//!
//! Selection has already split each input to the client's per-input limit. Batching does not
//! split text again or decide which cached vectors can be reused. `request` consumes a batch and
//! returns it with the service outcome, so a failed request still has its chunk count for
//! reporting. The scheduler owns concurrency and cancellation; the execution writer owns
//! persistence.
//!
//! Nearby tests distinguish count limits from byte limits. Workflow integration cases cover
//! provider failures and durable reuse rather than duplicating this deterministic grouping logic.

use forgesync_core::embedding::EmbeddingVector;
use tokio_util::sync::CancellationToken;

use crate::embedding_client::{EmbeddingClient, EmbeddingClientError};
use crate::embeddings::selection::EmbeddingTask;

/// Ordered pending inputs constrained by both service count and aggregate byte budgets.
pub struct EmbeddingBatch {
    /// Input order must match returned vector order; persistence rejects count mismatches.
    pub tasks: Vec<EmbeddingTask>,
}

impl EmbeddingBatch {
    /// Requests vectors while retaining the corresponding inputs for persistence or diagnostics.
    pub async fn request(
        self,
        client: EmbeddingClient,
        cancellation: CancellationToken,
    ) -> (Self, Result<Vec<EmbeddingVector>, EmbeddingClientError>) {
        let input = self
            .tasks
            .iter()
            .map(|task| task.chunk.text.clone())
            .collect::<Vec<_>>();
        let result = client.embed(&input, &cancellation).await;
        (self, result)
    }
}

/// Groups pending inputs under both count and byte budgets, preserving selection order.
///
/// Byte accounting uses UTF-8 encoded length rather than character count. Empty input returns no
/// batches, and every supplied task appears exactly once in its original order.
/// The caller must supply nonzero limits and individually eligible inputs from selection.
/// This grouping helper does not reject or split an oversized task: an input that exceeds the
/// aggregate budget alone remains a singleton for the client's validation boundary to reject.
/// No request is sent until the scheduler invokes [`EmbeddingBatch::request`].
pub fn make_batches(
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
#[path = "tests.rs"]
mod tests;
