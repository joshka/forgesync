//! Deterministic UTF-8 chunk contracts.

use forgesync_core::embedding::EmbeddingVector;
use forgesync_store::embeddings::StoredEmbeddingChunk;

use crate::embeddings::chunks::{DocumentChunk, chunk_document, compatible_chunks};

#[test]
fn repeated_chunking_preserves_text_coordinates_and_hashes() {
    let text = "first phrase 🦀 and another very long phrase";
    let first = chunk_document(text, 16).expect("chunks");
    let second = chunk_document(text, 16).expect("repeat chunks");

    assert_eq!(first, second);
}

#[rstest::rstest]
#[case::whitespace_boundary("first phrase 🦀 and another very long phrase", 16, vec!["first phrase", "🦀 and", "another very", "long phrase"])]
#[case::multibyte_boundary("🦀🦀x", 4, vec!["🦀", "🦀", "x"])]
#[case::trimmed_empty(" \n\t ", 4, vec![])]
fn chunk_text_respects_utf8_and_whitespace_boundaries(
    #[case] text: &str,
    #[case] max_bytes: usize,
    #[case] expected: Vec<&str>,
) {
    let chunks = chunk_document(text, max_bytes).expect("chunks");

    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk.text.as_str())
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn chunk_hash_changes_when_the_chunk_position_changes() {
    let first = chunk_document("same", 16).expect("first");
    let later = chunk_document("prefix same", 6).expect("later");

    assert_ne!(first[0].hash, later[1].hash);
}

#[rstest::rstest]
#[case::declared_dimensions(Some(2))]
#[case::unspecified_dimensions(None)]
fn matching_chunk_satisfies_selected_work(#[case] dimensions: Option<u32>) {
    let chunks = chunk_document("source text", 16).expect("chunks");
    let existing = StoredEmbeddingChunk {
        index: chunks[0].index,
        count: chunks[0].count,
        chunk_hash: chunks[0].hash.clone(),
        vector: EmbeddingVector::new(vec![1.0, 0.5], None).expect("vector"),
    };

    let selection = compatible_chunks(vec![existing], chunks, dimensions);

    assert!(selection.pending.is_empty());
    assert_eq!(selection.skipped, 1);
}

#[rstest::rstest]
#[case::different_position(1, 1, "selected-hash", Some(2))]
#[case::different_count(0, 2, "selected-hash", Some(2))]
#[case::different_hash(0, 1, "other-hash", Some(2))]
#[case::different_dimensions(0, 1, "selected-hash", Some(3))]
fn incompatible_chunk_remains_pending_without_reconstruction(
    #[case] index: u32,
    #[case] count: u32,
    #[case] hash: &str,
    #[case] dimensions: Option<u32>,
) {
    let chunk = DocumentChunk {
        index: 0,
        count: 1,
        hash: "selected-hash".to_owned(),
        text: "exact selected input".to_owned(),
    };
    let existing = StoredEmbeddingChunk {
        index,
        count,
        chunk_hash: hash.to_owned(),
        vector: EmbeddingVector::new(vec![1.0, 0.5], None).expect("vector"),
    };

    let selection = compatible_chunks(vec![existing], vec![chunk.clone()], dimensions);

    assert_eq!(selection.pending, vec![chunk]);
    assert_eq!(selection.skipped, 0);
}
