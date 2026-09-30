//! # Deterministic model inputs and stored-chunk compatibility
//!
//! `chunk_document` splits text at UTF-8-safe boundaries under the configured input byte budget.
//! `DocumentChunk` carries its position, total count, content hash, and exact model input text.
//! `compatible_chunks` compares these new inputs with the store's service-scoped chunk records and
//! separates reusable vectors from pending inputs.
//!
//! Hashing includes a versioned domain and chunk position. Whitespace normalization and positional
//! identity therefore affect reuse even when fragments look similar. Compatible stored vectors
//! must also have the same total chunk count and configured dimensions when dimensions are
//! explicit. The archive query already establishes document, recipe, endpoint, and model identity;
//! this module owns the remaining per-chunk checks. It performs no provider calls or archive
//! writes.

use forgesync_store::embeddings::StoredEmbeddingChunk;
use sha2::{Digest, Sha256};

use crate::error::EngineError;

/// One deterministic model input within a versioned, ordered document split.
/// Its hash identifies text and position; total count is checked independently during reuse.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentChunk {
    /// Zero-based position within this document split.
    pub index: u32,
    /// Total nonempty chunks in the complete split.
    pub count: u32,
    /// Versioned SHA-256 digest of length-delimited chunk position and text.
    pub hash: String,
    /// Exact normalized UTF-8 input sent to the model service.
    pub text: String,
}

/// Selects chunks matching the current document and model identity.
///
/// The caller supplies records already scoped to the document, recipe, endpoint, and model.
/// Reuse additionally requires the selected total count, optional dimensions, position, and hash.
/// Pending inputs retain their original order; each reused input increments `skipped` once.
/// This function checks no archive freshness and neither requests nor writes vectors.
pub fn compatible_chunks(
    existing: Vec<StoredEmbeddingChunk>,
    chunks: Vec<DocumentChunk>,
    count: u32,
    expected_dimensions: Option<u32>,
) -> CompatibleChunks {
    let current = existing
        .into_iter()
        .filter(|stored| {
            stored.count == count
                && expected_dimensions.is_none_or(|expected| stored.vector.dimensions() == expected)
        })
        .map(|stored| (stored.index, stored.chunk_hash))
        .collect::<std::collections::HashMap<_, _>>();
    let mut pending = Vec::new();
    let mut skipped = 0usize;
    for chunk in chunks {
        let index = chunk.index;
        let count = chunk.count;
        let hash = chunk.hash.clone();
        if current.get(&index).is_some_and(|stored| stored == &hash) {
            skipped = skipped.saturating_add(1);
        } else {
            pending.push(DocumentChunk {
                index,
                count,
                hash,
                text: chunk.text,
            });
        }
    }
    CompatibleChunks { pending, skipped }
}

/// Reuse selection that keeps pending input identity separate from already satisfied work.
pub struct CompatibleChunks {
    /// New or changed inputs whose compatible vectors are absent.
    pub pending: Vec<DocumentChunk>,
    /// Inputs satisfied by stored matching position, hash, count, and dimension records.
    pub skipped: usize,
}

/// Splits one document into deterministic model inputs.
///
/// Trims outer whitespace and prefers the last whitespace boundary within the byte budget.
/// When no such boundary exists, splitting uses the last whole UTF-8 character that fits.
/// Whitespace around each split is discarded; empty or whitespace-only input produces no chunks.
/// Every returned input has consecutive coordinates, a common count, and a text/position hash.
///
/// # Errors
///
/// Budgets below four bytes return [`EngineError::EmbeddingWorkerFailed`], since they cannot
/// accommodate every UTF-8 scalar. An unrepresentable chunk count or unusable boundary returns
/// [`EngineError::InvalidEmbeddingInput`]. No model request or archive operation runs here.
pub fn chunk_document(text: &str, max_bytes: usize) -> Result<Vec<DocumentChunk>, EngineError> {
    if max_bytes < 4 {
        return Err(EngineError::EmbeddingWorkerFailed);
    }
    let mut remaining = text.trim();
    let mut chunks = Vec::new();
    while !remaining.is_empty() {
        if remaining.len() <= max_bytes {
            chunks.push(remaining.to_owned());
            break;
        }
        let mut boundary = 0usize;
        for (index, character) in remaining.char_indices() {
            let next = index + character.len_utf8();
            if next > max_bytes {
                break;
            }
            boundary = next;
        }
        if boundary == 0 {
            return Err(EngineError::InvalidEmbeddingInput);
        }
        let split = remaining[..boundary]
            .char_indices()
            .rev()
            .find(|(_, character)| character.is_whitespace())
            .map(|(index, _)| index)
            .filter(|index| *index > 0)
            .unwrap_or(boundary);
        chunks.push(remaining[..split].trim_end().to_owned());
        remaining = remaining[split..].trim_start();
    }
    chunks.retain(|chunk| !chunk.is_empty());
    let count = u32::try_from(chunks.len()).map_err(|_| EngineError::InvalidEmbeddingInput)?;
    Ok(chunks
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let index = u32::try_from(index).expect("chunk count fits u32");
            DocumentChunk {
                index,
                count,
                hash: chunk_hash(index, &text),
                text,
            }
        })
        .collect())
}

/// Hashes the versioned domain, position, and text with length framing for reuse decisions.
///
/// Recipe and service identity are established by the caller's archive lookup, not this digest.
fn chunk_hash(index: u32, text: &str) -> String {
    let mut hasher = Sha256::new();
    add_hash_field(&mut hasher, b"forgesync-embedding-chunk-v1");
    add_hash_field(&mut hasher, &index.to_be_bytes());
    add_hash_field(&mut hasher, text.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Adds a length-delimited field to the stable chunk hash.
fn add_hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(value);
}

#[cfg(test)]
#[path = "chunk_tests.rs"]
mod tests;
