//! # Embedding request batch limits
//!
//! Named cases check input-count and UTF-8 byte limits independently and together using static
//! tasks. The source document fixture supplies normalized identity and text without provider or
//! archive I/O. Request batching groups already selected chunks; it does not choose cache
//! compatibility or regenerate their content hashes.
//!
//! Each expected batch size exposes one count/byte boundary without nested assertion logic.
//! Document chunking has its own nearby cases under `chunks`, while integration scenarios establish
//! retained successful batches and retry. Scheduling cancellation and writer-fence behavior remain
//! workflow contracts.

use std::sync::Arc;

use rstest::rstest;

use crate::embeddings::batches::make_batches;
use crate::embeddings::chunks::DocumentChunk;
use crate::embeddings::selection::EmbeddingTask;

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
    let counts = batches
        .iter()
        .map(|batch| batch.tasks.len())
        .collect::<Vec<_>>();

    assert_eq!(counts, expected_counts);
}

/// Constructs one four-byte input with an explicit position in the five-chunk fixture.
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
