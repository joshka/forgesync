//! # Deterministic UTF-8 chunk contracts
//!
//! These cases exercise text splitting directly, without document acquisition or a service client.
//! Repeated splitting must preserve boundaries, positional identity, and byte-budget constraints.
//! Chunk hashes include their position, so identical text moved to another chunk is not reusable.
//!
//! The fixtures contain whitespace and a multibyte character to keep byte-versus-character behavior
//! visible. Batch scheduling and database reuse are separate concerns in neighboring tests.
//! Each operation returns explicit chunks; assertions inspect their text and identity rather than
//! relying on a hidden scenario helper or provider fixture.

use crate::embeddings::chunks::chunk_document;

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
