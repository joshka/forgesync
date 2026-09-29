//! # Embedding request batch limits
//!
//! The batch case checks both input-count and UTF-8 byte limits using static document tasks.
//! The source document fixture supplies normalized identity and text without provider or archive
//! I/O. Request batching groups already selected chunks; it does not choose cache compatibility or
//! regenerate their content hashes.
//!
//! The expected batch sizes expose the count/byte boundary. Document chunking has its own nearby
//! cases under `chunks`, while integration scenarios establish retained successful batches and
//! retry. Scheduling cancellation and writer-fence behavior remain workflow contracts.

use std::sync::Arc;

use crate::embeddings::chunks::DocumentChunk;
use crate::embeddings::make_batches;
use crate::embeddings::selection::EmbeddingTask;

#[test]
fn request_batches_obey_count_and_combined_byte_limits() {
    let tasks = (0..5)
        .map(|index| EmbeddingTask {
            document: Arc::new(test_document()),
            chunk: DocumentChunk {
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
