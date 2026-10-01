//! Request batch limits and worker draining.

use std::sync::Arc;

use rstest::rstest;
use tokio::sync::oneshot;
use tokio::task::JoinSet;

use crate::embeddings::chunks::DocumentChunk;
use crate::embeddings::{BatchResponse, EmbeddingTask, drain, make_batches};

#[rstest]
#[case::input_count(2, 40, &[2, 2, 1])]
#[case::combined_bytes(5, 8, &[2, 2, 1])]
#[case::both_limits(2, 8, &[2, 2, 1])]
fn request_batches_obey_count_and_combined_byte_limits(
    #[case] max_inputs: usize,
    #[case] max_bytes: usize,
    #[case] expected_counts: &[usize],
) {
    let document = Arc::new(test_document());
    let tasks = vec![
        test_task(Arc::clone(&document), 0),
        test_task(Arc::clone(&document), 1),
        test_task(Arc::clone(&document), 2),
        test_task(Arc::clone(&document), 3),
        test_task(document, 4),
    ];

    let batches = make_batches(tasks, max_inputs, max_bytes);
    let counts = batches.iter().map(Vec::len).collect::<Vec<_>>();

    assert_eq!(counts, expected_counts);
    let positions = batches
        .iter()
        .flat_map(|batch| batch.iter().map(|task| task.chunk.index))
        .collect::<Vec<_>>();
    assert_eq!(positions, [0, 1, 2, 3, 4]);
}

#[test]
fn aggregate_limit_counts_utf8_bytes_rather_than_characters() {
    let document = Arc::new(test_document());
    let mut first = test_task(Arc::clone(&document), 0);
    first.chunk.text = "🦀".to_owned();
    let mut second = test_task(document, 1);
    second.chunk.text = "🐙".to_owned();

    let batches = make_batches(vec![first, second], 2, 4);

    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0].len(), 1);
    assert_eq!(batches[0][0].chunk.text, "🦀");
    assert_eq!(batches[1].len(), 1);
    assert_eq!(batches[1][0].chunk.text, "🐙");
}

#[tokio::test]
async fn drain_waits_for_pending_workers_to_drop_their_resources() {
    let mut workers = JoinSet::<BatchResponse>::new();
    let (resource, mut released) = oneshot::channel::<()>();
    workers.spawn(async move {
        let _resource = resource;
        std::future::pending().await
    });

    drain(&mut workers).await;

    assert!(workers.is_empty());
    assert_eq!(
        released.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    );
}

/// Constructs one four-byte input with an explicit position in the five-chunk fixture.
///
/// Hashes are synthetic because batching does not validate hashes or complete chunk membership.
fn test_task(document: Arc<forgesync_core::document::Document>, index: u32) -> EmbeddingTask {
    EmbeddingTask {
        document,
        chunk: DocumentChunk {
            index,
            count: 5,
            hash: format!("{index:064x}"),
            text: "four".to_owned(),
        },
    }
}

/// Constructs a static normalized source document without service or archive setup.
fn test_document() -> forgesync_core::document::Document {
    use forgesync_core::document::{Document, DocumentRecipe};
    use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
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
