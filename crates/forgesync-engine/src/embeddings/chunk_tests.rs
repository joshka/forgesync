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
fn chunks_are_deterministic_utf8_safe_and_within_the_byte_budget() {
    let text = "first phrase 🦀 and another very long phrase";
    let first = chunk_document(text, 16).expect("chunks");
    let second = chunk_document(text, 16).expect("repeat chunks");

    assert_eq!(first.len(), second.len());
    assert!(first.iter().all(|chunk| chunk.text.len() <= 16));
    assert!(first.iter().all(|chunk| chunk.hash.len() == 64));
    assert_eq!(
        first
            .iter()
            .map(|chunk| chunk.text.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        text
    );
    assert_eq!(first, second);
}

#[test]
fn chunk_hash_changes_when_the_chunk_position_changes() {
    let first = chunk_document("same", 16).expect("first");
    let later = chunk_document("prefix same", 6).expect("later");

    assert_ne!(first[0].hash, later[1].hash);
}
