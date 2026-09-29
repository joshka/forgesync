//! Embedding chunks, search inputs, and write operations.

use std::collections::HashMap;
use std::num::NonZeroU32;

use forgesync_core::content::ThreadKind;
use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_core::embedding::EmbeddingVector;
use forgesync_core::identity::RepositoryId;
use forgesync_core::timestamp::UtcTimestamp;
use serde::Serialize;
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::reads::{
    ThreadStateFilter, ThreadSummary, coverage_for_kind, load_thread_coverage,
    push_discussion_filters, push_repository_scope,
};

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
    /// Reads one bounded page of current documents with complete embedding chunks.
    pub async fn embedding_search_page(
        &self,
        query: &EmbeddingDocumentQuery<'_>,
    ) -> Result<EmbeddingDocumentPage, StoreError> {
        let mut statement = QueryBuilder::<Sqlite>::new(
            "SELECT DISTINCT d.id AS document_id, t.id AS thread_id, r.payload_json AS repository_json, t.payload_json AS discussion_json FROM embeddings e JOIN documents d ON d.id = e.document_id JOIN threads t ON t.id = d.thread_id JOIN repositories r ON r.id = t.repository_id WHERE e.endpoint = ",
        );
        statement
            .push_bind(query.endpoint)
            .push(" AND e.model = ")
            .push_bind(query.model)
            .push(" AND e.document_hash = d.content_hash AND d.recipe = ")
            .push_bind(query.recipe.as_str())
            .push(" AND d.recipe_version = ")
            .push_bind(i64::from(DocumentRecipe::VERSION))
            .push(" AND d.source_updated_at_us = t.updated_at_us");
        if query.recipe == DocumentRecipe::DiscussionEnriched {
            statement.push(
                " AND d.built_at_us >= COALESCE((SELECT MAX(c.observed_at_us) FROM family_coverage c WHERE c.thread_id = t.id AND c.family IN ('comments', 'reviews', 'review_threads')), 0)",
            );
        }
        push_repository_scope(&mut statement, query.repositories);
        push_discussion_filters(&mut statement, query.kind, query.state);
        if let Some(after_document_id) = query.after_document_id {
            statement.push(" AND d.id > ").push_bind(after_document_id);
        }
        let limit = i64::from(query.limit.get());
        statement.push(" ORDER BY d.id LIMIT ").push_bind(limit);
        let candidates = statement.build().fetch_all(&self.reader).await?;
        if candidates.is_empty() {
            return Ok(EmbeddingDocumentPage {
                items: Vec::new(),
                next_document_id: None,
            });
        }

        let mut candidate_rows = Vec::with_capacity(candidates.len());
        let mut document_ids = Vec::with_capacity(candidates.len());
        let mut thread_ids = Vec::with_capacity(candidates.len());
        for row in candidates {
            let document_id: i64 = row.try_get("document_id")?;
            let thread_id: i64 = row.try_get("thread_id")?;
            document_ids.push(document_id);
            thread_ids.push(thread_id);
            candidate_rows.push((
                document_id,
                thread_id,
                row.try_get::<String, _>("repository_json")?,
                row.try_get::<String, _>("discussion_json")?,
            ));
        }
        let coverage_by_thread = load_thread_coverage(&self.reader, &thread_ids).await?;
        let mut summaries = HashMap::with_capacity(candidate_rows.len());
        for (_, thread_id, repository_json, discussion_json) in &candidate_rows {
            let repository = serde_json::from_str(repository_json)?;
            let discussion = serde_json::from_str(discussion_json)?;
            let coverage = coverage_for_kind(&discussion, coverage_by_thread.get(thread_id));
            if query.recipe == DocumentRecipe::DiscussionEnriched
                && coverage.iter().any(|coverage| coverage.is_stale())
            {
                continue;
            }
            summaries.insert(
                *thread_id,
                ThreadSummary {
                    repository,
                    discussion,
                    coverage,
                },
            );
        }

        let mut vector_statement = QueryBuilder::<Sqlite>::new(
            "SELECT e.document_id, e.chunk_index, e.chunk_count, e.chunk_hash, e.dimensions, e.vector_le FROM embeddings e JOIN documents d ON d.id = e.document_id WHERE e.endpoint = ",
        );
        vector_statement
            .push_bind(query.endpoint)
            .push(" AND e.model = ")
            .push_bind(query.model)
            .push(" AND e.document_hash = d.content_hash AND d.recipe = ")
            .push_bind(query.recipe.as_str())
            .push(" AND d.recipe_version = ")
            .push_bind(i64::from(DocumentRecipe::VERSION))
            .push(" AND e.document_id IN (");
        for (index, document_id) in document_ids.iter().enumerate() {
            if index > 0 {
                vector_statement.push(", ");
            }
            vector_statement.push_bind(document_id);
        }
        vector_statement.push(") ORDER BY e.document_id, e.chunk_index");
        let vector_rows = vector_statement.build().fetch_all(&self.reader).await?;
        let mut chunks_by_document: HashMap<i64, Vec<StoredEmbeddingChunk>> = HashMap::new();
        let mut invalid_documents = std::collections::HashSet::new();
        for row in vector_rows {
            let document_id: i64 = row.try_get("document_id")?;
            match decode_embedding_chunk(row) {
                Ok(chunk) => chunks_by_document
                    .entry(document_id)
                    .or_default()
                    .push(chunk),
                Err(_) => {
                    invalid_documents.insert(document_id);
                }
            }
        }

        let mut items = Vec::with_capacity(candidate_rows.len());
        for (document_id, thread_id, _, _) in &candidate_rows {
            let Some(summary) = summaries.remove(thread_id) else {
                continue;
            };
            let Some(chunks) = chunks_by_document.remove(document_id) else {
                continue;
            };
            if invalid_documents.contains(document_id) || !complete_chunk_set(&chunks) {
                continue;
            }
            items.push(EmbeddingSearchDocument { summary, chunks });
        }
        let last_document_id = candidate_rows.last().map(|(document_id, ..)| *document_id);
        let next_document_id = (candidate_rows.len()
            == usize::try_from(query.limit.get()).unwrap_or(usize::MAX))
        .then_some(last_document_id)
        .flatten();

        Ok(EmbeddingDocumentPage {
            items,
            next_document_id,
        })
    }

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

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
