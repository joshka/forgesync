//! # Persist embedding chunks and select compatible work
//!
//! `EmbeddingWrite` and `EmbeddingChunkInput` carry vectors generated for a document.
//! `StoredEmbeddingChunk` is the local read form. `EmbeddingDocumentQuery`,
//! `EmbeddingSearchDocument`, and `EmbeddingDocumentPage` select documents that need or can use
//! embeddings.
//!
//! Embedding data is derived from a document, model identity, and dimensions. Reads must keep
//! incompatible vectors out of a search, while writes must associate chunks with the correct
//! document version. The engine owns service calls and batching; this module owns their durable
//! local result.

use std::num::NonZeroU32;

use forgesync_core::content::ThreadKind;
use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_core::embedding::EmbeddingVector;
use forgesync_core::identity::RepositoryId;
use forgesync_core::timestamp::UtcTimestamp;
use serde::Serialize;
use sqlx::Row;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::reads::{ThreadStateFilter, ThreadSummary};

mod read;

/// One current embedding chunk read from the archive.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredEmbeddingChunk {
    /// Zero-based position in the deterministic document split.
    pub index: u32,
    /// Number of chunks in the split that produced this vector.
    pub count: u32,
    /// Hash of the exact text sent for this chunk.
    pub chunk_hash: String,
    /// Validated vector components.
    pub vector: EmbeddingVector,
}

/// Filters and keyset cursor for bounded semantic-search reads.
pub struct EmbeddingDocumentQuery<'a> {
    /// Resolved repository scope; an empty list includes every repository.
    pub repositories: &'a [RepositoryId],
    /// Optional issue or pull-request kind.
    pub kind: Option<ThreadKind>,
    /// Current source-state filter.
    pub state: ThreadStateFilter,
    /// Exact endpoint identity used when the vectors were stored.
    pub endpoint: &'a str,
    /// Exact model identity used when the vectors were stored.
    pub model: &'a str,
    /// Current configured document recipe.
    pub recipe: DocumentRecipe,
    /// Last document row ID returned by the previous page.
    pub after_document_id: Option<i64>,
    /// Maximum number of documents in this page.
    pub limit: NonZeroU32,
}

/// A current document and all of its complete, compatible stored chunks.
#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddingSearchDocument {
    /// Current discussion and coverage used to build the document.
    pub summary: ThreadSummary,
    /// Stored embedding chunks in document order.
    pub chunks: Vec<StoredEmbeddingChunk>,
}

/// One bounded page of documents with compatible stored embeddings.
#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddingDocumentPage {
    /// Current documents with complete chunk coverage.
    pub items: Vec<EmbeddingSearchDocument>,
    /// Keyset cursor for the next page, when more candidates may exist.
    pub next_document_id: Option<i64>,
}

/// Validated input used to persist one model response.
pub struct EmbeddingChunkInput<'a> {
    /// Canonical provider endpoint identity without credentials.
    pub endpoint: &'a str,
    /// Configured model name.
    pub model: &'a str,
    /// Zero-based position in the deterministic document split.
    pub index: u32,
    /// Number of chunks in the split that produced this vector.
    pub count: u32,
    /// Hash of the exact text sent for this chunk.
    pub chunk_hash: &'a str,
    /// Validated vector components.
    pub vector: &'a EmbeddingVector,
}

/// Result of persisting one current embedding chunk.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EmbeddingWrite {
    /// Stable archive-local embedding row ID.
    pub id: i64,
}

impl Archive {
    /// Lists persisted chunks matching the current document and service identity.
    pub async fn embedding_chunks(
        &self,
        document: &Document,
        endpoint: &str,
        model: &str,
        chunk_count: u32,
    ) -> Result<Vec<StoredEmbeddingChunk>, StoreError> {
        if document.recipe_version != DocumentRecipe::VERSION
            || document.content_hash != document.expected_content_hash()
        {
            return Err(StoreError::InvalidDocument);
        }
        if chunk_count == 0 || endpoint.trim().is_empty() || model.trim().is_empty() {
            return Ok(Vec::new());
        }
        let current_hash: Option<String> = sqlx::query_scalar(
            "SELECT d.content_hash FROM documents d JOIN threads t ON t.id = d.thread_id JOIN repositories r ON r.id = t.repository_id WHERE r.host = ? AND r.provider_id = ? AND t.provider_id = ? AND t.number = ? AND d.recipe = ? AND d.recipe_version = ?",
        )
        .bind(document.source_identity.repository().host().as_str())
        .bind(document.source_identity.repository().provider_id().as_str())
        .bind(document.source_identity.provider_id().as_str())
        .bind(i64::try_from(document.source_identity.number().get()).map_err(|_| StoreError::IntegerOutOfRange)?)
        .bind(document.recipe.as_str())
        .bind(i64::from(document.recipe_version))
        .fetch_optional(&self.reader)
        .await?;
        if current_hash.as_deref() != Some(document.content_hash.as_str()) {
            return Err(StoreError::DocumentNotCurrent);
        }
        let rows = sqlx::query(
            "SELECT e.chunk_index, e.chunk_count, e.chunk_hash, e.dimensions, e.vector_le FROM embeddings e JOIN documents d ON d.id = e.document_id JOIN threads t ON t.id = d.thread_id JOIN repositories r ON r.id = t.repository_id WHERE r.host = ? AND r.provider_id = ? AND t.provider_id = ? AND t.number = ? AND d.recipe = ? AND d.content_hash = ? AND e.endpoint = ? AND e.model = ? AND e.document_hash = d.content_hash AND e.chunk_count = ? ORDER BY e.chunk_index",
        )
        .bind(document.source_identity.repository().host().as_str())
        .bind(document.source_identity.repository().provider_id().as_str())
        .bind(document.source_identity.provider_id().as_str())
        .bind(i64::try_from(document.source_identity.number().get()).map_err(|_| StoreError::IntegerOutOfRange)?)
        .bind(document.recipe.as_str())
        .bind(&document.content_hash)
        .bind(endpoint)
        .bind(model)
        .bind(i64::from(chunk_count))
        .fetch_all(&self.reader)
        .await?;

        rows.into_iter().map(decode_embedding_chunk).collect()
    }

    /// Stores a vector only while its source document and the archive writer fence are current.
    pub async fn upsert_embedding_chunk_fenced(
        &self,
        token: &ArchiveLeaseToken,
        document: &Document,
        chunk: &EmbeddingChunkInput<'_>,
        stored_at: UtcTimestamp,
    ) -> Result<EmbeddingWrite, StoreError> {
        if document.recipe_version != DocumentRecipe::VERSION
            || document.content_hash != document.expected_content_hash()
            || !is_sha256_hex(&document.content_hash)
            || !is_sha256_hex(chunk.chunk_hash)
            || chunk.endpoint.trim().is_empty()
            || chunk.model.trim().is_empty()
            || chunk.count == 0
            || chunk.index >= chunk.count
        {
            return Err(StoreError::InvalidEmbedding);
        }
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let Some(document_row_id) = current_document_row_id(&mut transaction, document).await?
        else {
            return Err(StoreError::DocumentNotCurrent);
        };
        let vector_bytes = chunk.vector.to_little_endian();
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO embeddings (document_id, endpoint, model, document_hash, chunk_index, chunk_count, chunk_hash, dimensions, vector_le, created_at_us, updated_at_us) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (document_id, endpoint, model, document_hash, chunk_index) DO UPDATE SET chunk_count = excluded.chunk_count, chunk_hash = excluded.chunk_hash, dimensions = excluded.dimensions, vector_le = excluded.vector_le, updated_at_us = excluded.updated_at_us RETURNING id",
        )
        .bind(document_row_id)
        .bind(chunk.endpoint)
        .bind(chunk.model)
        .bind(&document.content_hash)
        .bind(i64::from(chunk.index))
        .bind(i64::from(chunk.count))
        .bind(chunk.chunk_hash)
        .bind(i64::from(chunk.vector.dimensions()))
        .bind(vector_bytes)
        .bind(stored_at.unix_microseconds())
        .bind(stored_at.unix_microseconds())
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(EmbeddingWrite { id })
    }
}

/// Accepts a stored document's vectors only when every indexed chunk is present once and all
/// chunks agree on count and dimensions; a partial embedding must not enter semantic search.
fn complete_chunk_set(chunks: &[StoredEmbeddingChunk]) -> bool {
    let Some(first) = chunks.first() else {
        return false;
    };
    let Ok(expected_count) = usize::try_from(first.count) else {
        return false;
    };
    if expected_count == 0 || chunks.len() != expected_count {
        return false;
    }
    chunks.iter().enumerate().all(|(position, chunk)| {
        chunk.count == first.count
            && chunk.vector.dimensions() == first.vector.dimensions()
            && usize::try_from(chunk.index) == Ok(position)
    })
}

/// Finds the current document version before attaching vector chunks.
async fn current_document_row_id(
    connection: &mut sqlx::SqliteConnection,
    document: &Document,
) -> Result<Option<i64>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT d.id FROM documents d JOIN threads t ON t.id = d.thread_id JOIN repositories r ON r.id = t.repository_id WHERE r.host = ? AND r.provider_id = ? AND t.provider_id = ? AND t.number = ? AND d.recipe = ? AND d.recipe_version = ? AND d.content_hash = ?",
    )
    .bind(document.source_identity.repository().host().as_str())
    .bind(document.source_identity.repository().provider_id().as_str())
    .bind(document.source_identity.provider_id().as_str())
    .bind(i64::try_from(document.source_identity.number().get()).map_err(|_| StoreError::IntegerOutOfRange)?)
    .bind(document.recipe.as_str())
    .bind(i64::from(document.recipe_version))
    .bind(&document.content_hash)
    .fetch_optional(&mut *connection)
    .await?)
}

/// Validates stored dimensions and bytes before yielding a search vector.
fn decode_embedding_chunk(
    row: sqlx::sqlite::SqliteRow,
) -> Result<StoredEmbeddingChunk, StoreError> {
    let index = u32::try_from(row.try_get::<i64, _>("chunk_index")?)
        .map_err(|_| StoreError::InvalidEmbedding)?;
    let count = u32::try_from(row.try_get::<i64, _>("chunk_count")?)
        .map_err(|_| StoreError::InvalidEmbedding)?;
    let dimensions = u32::try_from(row.try_get::<i64, _>("dimensions")?)
        .map_err(|_| StoreError::InvalidEmbedding)?;
    let bytes: Vec<u8> = row.try_get("vector_le")?;
    let vector = EmbeddingVector::from_little_endian(&bytes, dimensions)
        .map_err(|_| StoreError::InvalidEmbedding)?;
    if count == 0 || index >= count {
        return Err(StoreError::InvalidEmbedding);
    }
    let chunk_hash: String = row.try_get("chunk_hash")?;
    if !is_sha256_hex(&chunk_hash) {
        return Err(StoreError::InvalidEmbedding);
    }
    Ok(StoredEmbeddingChunk {
        index,
        count,
        chunk_hash,
        vector,
    })
}

/// Checks a persisted chunk hash before using it as a cache identity.
fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
