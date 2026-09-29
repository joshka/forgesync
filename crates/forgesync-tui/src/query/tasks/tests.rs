//! # Query lifetime contracts
//!
//! These cases exercise the owner directly with controlled tasks. Cancellation is observed through
//! the same token a writer receives; completed reads are pruned before a subsequent read is
//! tracked. Shutdown waits for a cancelled writer rather than aborting its cleanup work.

use tokio_util::sync::CancellationToken;

use crate::query::tasks::QueryTasks;

#[tokio::test]
async fn repeated_cancellation_signals_the_writer() {
    let cancellation = CancellationToken::new();
    let writer_token = cancellation.clone();
    let handle = tokio::spawn(async move { writer_token.cancelled().await });
    let mut tasks = QueryTasks::default();
    tasks.track_operation(handle, cancellation.clone());

    tasks.cancel_operation();
    tasks.cancel_operation();

    assert!(cancellation.is_cancelled());
    tasks.stop().await;
    assert!(tasks.operation.is_none());
}

#[tokio::test]
async fn shutdown_waits_for_writer_cleanup() {
    let cancellation = CancellationToken::new();
    let writer_token = cancellation.clone();
    let (finished, completion) = tokio::sync::oneshot::channel();
    let handle = tokio::spawn(async move {
        writer_token.cancelled().await;
        finished.send(()).expect("cleanup receiver remains alive");
    });
    let mut tasks = QueryTasks::default();
    tasks.track_operation(handle, cancellation);

    tasks.stop().await;

    completion.await.expect("writer completed cleanup");
    assert!(tasks.operation.is_none());
}

#[tokio::test]
async fn tracking_a_read_prunes_completed_reads() {
    let mut finished = tokio::spawn(async {});
    (&mut finished).await.expect("read finished");
    let mut tasks = QueryTasks::default();
    tasks.push(finished);
    let pending = tokio::spawn(std::future::pending());

    tasks.push(pending);

    assert_eq!(tasks.handles.len(), 1);
    tasks.stop().await;
    assert!(tasks.handles.is_empty());
}
