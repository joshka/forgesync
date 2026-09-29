//! # Progress lifetime and completion ordering
//!
//! These cases use bounded channels directly rather than a terminal fixture. They verify that
//! buffered progress arrives before completion, drop closes the producer's receiver, and a closed
//! UI destination permits finishing without changing the operation result.

use std::time::Duration;

use forgesync_core::identity::RunId;
use forgesync_engine::sync::{SyncProgress, SyncProgressStatus};
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::app::messages::QueryMessage;
use crate::query::progress::ProgressForwarder;

#[tokio::test]
async fn buffered_progress_precedes_terminal_result() {
    let (destination, mut messages) = mpsc::channel(4);
    let forwarder = ProgressForwarder::start(7, destination.clone());
    let producer = forwarder.sender();
    let snapshot = sample_progress();
    producer
        .send(snapshot.clone())
        .await
        .expect("progress accepted");
    drop(producer);

    forwarder.finish().await;
    destination
        .send(QueryMessage::OperationFinished {
            generation: 7,
            result: Ok("Sync complete".to_owned()),
        })
        .await
        .expect("completion accepted");

    let first = messages.recv().await.expect("progress delivered");
    let last = messages.recv().await.expect("completion delivered");
    assert!(
        matches!(first, QueryMessage::OperationProgress { generation: 7, progress } if progress == snapshot)
    );
    assert!(matches!(
        last,
        QueryMessage::OperationFinished {
            generation: 7,
            result: Ok(_)
        }
    ));
}

#[tokio::test]
async fn finishing_without_events_closes_the_owned_sender() {
    let (destination, _messages) = mpsc::channel(4);
    let forwarder = ProgressForwarder::start(7, destination);

    timeout(Duration::from_secs(5), forwarder.finish())
        .await
        .expect("idle forwarder drains");
}

#[tokio::test]
async fn dropping_owner_closes_delivery_to_producer_clones() {
    let (destination, _messages) = mpsc::channel(4);
    let forwarder = ProgressForwarder::start(7, destination);
    let producer = forwarder.sender();

    drop(forwarder);

    timeout(Duration::from_secs(5), producer.closed())
        .await
        .expect("aborted receiver closes");
}

#[tokio::test]
async fn closed_terminal_channel_does_not_prevent_finishing() {
    let (destination, messages) = mpsc::channel(4);
    let forwarder = ProgressForwarder::start(7, destination);
    let producer = forwarder.sender();
    drop(messages);
    producer
        .send(sample_progress())
        .await
        .expect("progress accepted before forwarding");
    drop(producer);

    timeout(Duration::from_secs(5), forwarder.finish())
        .await
        .expect("closed terminal ends delivery");
}

/// A static committed-job snapshot; no execution or cancellation behavior is hidden in the fixture.
fn sample_progress() -> SyncProgress {
    SyncProgress {
        run_id: RunId::new(23).expect("run identity"),
        completed_jobs: 1,
        total_jobs: 1,
        threads_seen: 3,
        comments_seen: 0,
        pull_request_metadata_seen: 0,
        reviews_seen: 0,
        review_threads_seen: 0,
        repository: Some("https://github.com/owner/repo".to_owned()),
        status: SyncProgressStatus::Complete,
    }
}
