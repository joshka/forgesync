//! # Worker cleanup before returning a fatal embedding error
//!
//! The scheduler must drain aborted tasks, rather than merely request their cancellation.
//! This regression case puts a retained resource in a pending worker and checks its release
//! immediately after fatal completion. The original worker error must remain the returned error.
//!
//! The oneshot sender represents a resource captured by an outstanding request. Its receiver
//! becomes closed when the worker future is dropped. No provider request, archive, timing delay,
//! or polling loop is needed to establish this lifetime boundary.
//!
//! End-to-end cases cover durable vectors and retry. This case isolates the scheduler guarantee
//! that those callers rely on before releasing their writer fence.

use std::time::Duration;

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::embedding_client::{EmbeddingClient, EmbeddingClientConfig};
use crate::embeddings::EmbeddingReport;
use crate::embeddings::scheduling::{BatchResponse, BatchScheduler};
use crate::error::EngineError;

#[tokio::test]
async fn fatal_completion_drains_pending_workers_and_preserves_the_error() {
    let client = test_client();
    let cancellation = CancellationToken::new();
    let mut scheduler = BatchScheduler::new(Vec::new(), &client, &cancellation);
    let (resource, mut released) = oneshot::channel::<()>();
    scheduler.workers.spawn(async move {
        let _resource = resource;
        std::future::pending::<BatchResponse>().await
    });

    let result = scheduler
        .finish(Err(EngineError::EmbeddingWorkerFailed))
        .await;

    assert!(matches!(result, Err(EngineError::EmbeddingWorkerFailed)));
    assert!(scheduler.workers.is_empty());
    assert_eq!(
        released.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    );
}

#[tokio::test]
async fn cancellation_drains_workers_and_retains_completed_chunk_counts() {
    let client = test_client();
    let cancellation = CancellationToken::new();
    let mut scheduler = BatchScheduler::new(Vec::new(), &client, &cancellation);
    let mut report = EmbeddingReport {
        chunks_embedded: 7,
        ..EmbeddingReport::default()
    };
    let (resource, mut released) = oneshot::channel::<()>();
    scheduler.workers.spawn(async move {
        let _resource = resource;
        std::future::pending::<BatchResponse>().await
    });

    let interruption = scheduler.cancel(&mut report);
    let result = scheduler.finish(Ok(())).await;

    assert!(interruption.is_break());
    assert!(result.is_ok());
    assert!(report.cancelled);
    assert_eq!(report.chunks_embedded, 7);
    assert!(scheduler.workers.is_empty());
    assert_eq!(
        released.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    );
}

/// Constructs a valid service capability without credentials from the process or network I/O.
fn test_client() -> EmbeddingClient {
    EmbeddingClient::new(EmbeddingClientConfig {
        endpoint: "http://127.0.0.1/v1".parse().expect("fixture endpoint"),
        model: "fixture-model".to_owned(),
        api_key: "fixture-key".to_owned(),
        dimensions: Some(2),
        max_input_bytes: 10,
        max_batch_input_bytes: 10,
        batch_size: 1,
        concurrency: 1,
        request_timeout: Duration::from_secs(2),
        total_budget: Duration::from_secs(3),
        max_attempts: 1,
    })
    .expect("fixture client")
}
