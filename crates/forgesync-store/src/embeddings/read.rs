//! # Read compatible semantic-search pages
//!
//! `EmbeddingDocumentQuery` binds service identity, document freshness, filters, and keyset cursor.
//! `EmbeddingCandidates` keeps selected row order while coverage and vectors are hydrated in
//! batches. `CompatibleChunks` rejects a whole document when any stored vector is invalid or the
//! chunk set is incomplete. These checks keep semantic search from using partial derived data.
//!
//! The next cursor comes from raw candidates, including those later filtered out. Otherwise an
//! invalid final candidate could prevent the caller from reaching valid documents on later pages.
//! `embeddings` owns vector writes and single-document reads; engine search owns similarity policy.
//!
//! These reads are bounded, local, and do not materialize documents or contact a model service.
//! Candidate payloads, coverage, and vectors are loaded in separate reads rather than one snapshot.
//! Concurrent writes can advance them between phases; this is a search projection, not proof that
//! every returned component describes one atomic archive revision.

use std::collections::{HashMap, HashSet};

use sqlx::{QueryBuilder, Row, Sqlite};

use super::{
    DocumentRecipe, EmbeddingDocumentPage, EmbeddingDocumentQuery, EmbeddingSearchDocument,
    StoredEmbeddingChunk, complete_chunk_set, decode_embedding_chunk,
};
use crate::archive::Archive;
use crate::coverage_projection::{coverage_for_kind, load_thread_coverage};
use crate::error::StoreError;
use crate::query_sql::{push_discussion_filters, push_repository_scope};
use crate::reads::ThreadSummary;

impl Archive {
    /// Reads current documents, then independently checks evidence and complete vector sets.
    ///
    /// The limit bounds raw candidates, so rejected vectors or stale enriched coverage can leave
    /// fewer results, including an empty page with a continuation cursor. Advance using the page's
    /// cursor rather than the last returned item. Candidate, coverage, and vector reads do not
    /// share a transaction snapshot. Invalid chunk data rejects its whole document; SQL
    /// failures and invalid candidate payload JSON still fail the operation.
    pub async fn embedding_search_page(
        &self,
        query: &EmbeddingDocumentQuery<'_>,
    ) -> Result<EmbeddingDocumentPage, StoreError> {
        let mut statement = query.candidates();
        let rows = statement.build().fetch_all(&self.reader).await?;
        let candidates = EmbeddingCandidates::decode(rows)?;
        if candidates.rows.is_empty() {
            return Ok(EmbeddingDocumentPage {
                items: Vec::new(),
                next_document_id: None,
            });
        }
        let summaries = candidates.summaries(self, query.recipe).await?;
        let document_ids = candidates
            .rows
            .iter()
            .map(|row| row.document_id)
            .collect::<Vec<_>>();
        let mut statement = query.vectors(&document_ids);
        let rows = statement.build().fetch_all(&self.reader).await?;
        let chunks = CompatibleChunks::decode(rows)?;
        Ok(candidates.page(summaries, chunks, query.limit.get()))
    }
}

impl EmbeddingDocumentQuery<'_> {
    /// Selects a bounded ordered page before post-query compatibility checks.
    fn candidates(&self) -> QueryBuilder<Sqlite> {
        let mut statement = QueryBuilder::<Sqlite>::new(
            "SELECT DISTINCT d.id AS document_id, t.id AS thread_id, r.payload_json AS repository_json, t.payload_json AS discussion_json FROM embeddings e JOIN documents d ON d.id = e.document_id JOIN threads t ON t.id = d.thread_id JOIN repositories r ON r.id = t.repository_id WHERE e.endpoint = ",
        );
        statement
            .push_bind(self.endpoint)
            .push(" AND e.model = ")
            .push_bind(self.model)
            .push(" AND e.document_hash = d.content_hash AND d.recipe = ")
            .push_bind(self.recipe.as_str())
            .push(" AND d.recipe_version = ")
            .push_bind(i64::from(DocumentRecipe::VERSION))
            .push(" AND d.source_updated_at_us = t.updated_at_us");
        if self.recipe == DocumentRecipe::DiscussionEnriched {
            statement.push(
                " AND d.built_at_us >= COALESCE((SELECT MAX(c.observed_at_us) FROM family_coverage c WHERE c.thread_id = t.id AND c.family IN ('comments', 'reviews', 'review_threads')), 0)",
            );
        }
        push_repository_scope(&mut statement, self.repositories);
        push_discussion_filters(&mut statement, self.kind, self.state);
        if let Some(after_document_id) = self.after_document_id {
            statement.push(" AND d.id > ").push_bind(after_document_id);
        }
        let limit = i64::from(self.limit.get());
        statement.push(" ORDER BY d.id LIMIT ").push_bind(limit);
        statement
    }

    /// Binds the service and current document identity for the selected candidates' vectors.
    fn vectors(&self, document_ids: &[i64]) -> QueryBuilder<Sqlite> {
        let mut vector_statement = QueryBuilder::<Sqlite>::new(
            "SELECT e.document_id, e.chunk_index, e.chunk_count, e.chunk_hash, e.dimensions, e.vector_le FROM embeddings e JOIN documents d ON d.id = e.document_id WHERE e.endpoint = ",
        );
        vector_statement
            .push_bind(self.endpoint)
            .push(" AND e.model = ")
            .push_bind(self.model)
            .push(" AND e.document_hash = d.content_hash AND d.recipe = ")
            .push_bind(self.recipe.as_str())
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
        vector_statement
    }
}

/// Persisted candidate identity and payload, prior to coverage and vector validation.
struct EmbeddingCandidate {
    /// Local document row identity used for keyset ordering and vector grouping.
    document_id: i64,
    /// Local parent row identity used to batch coverage reads and assemble summaries.
    thread_id: i64,
    /// Canonical repository payload captured by the candidate query, decoded during hydration.
    repository_json: String,
    /// Canonical discussion payload captured with the candidate, not reread with its vectors.
    discussion_json: String,
}
/// Ordered raw candidates whose last ID determines cursor advancement.
struct EmbeddingCandidates {
    /// Ascending document-ID candidates retained even when later hydration rejects an item.
    rows: Vec<EmbeddingCandidate>,
}
impl EmbeddingCandidates {
    /// Decodes row identities without deciding whether their vectors are usable.
    fn decode(rows: Vec<sqlx::sqlite::SqliteRow>) -> Result<Self, StoreError> {
        let rows = rows
            .into_iter()
            .map(|row| {
                Ok(EmbeddingCandidate {
                    document_id: row.try_get("document_id")?,
                    thread_id: row.try_get("thread_id")?,
                    repository_json: row.try_get("repository_json")?,
                    discussion_json: row.try_get("discussion_json")?,
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        Ok(Self { rows })
    }

    /// Hydrates current evidence and excludes stale enriched documents before ranking.
    async fn summaries(
        &self,
        archive: &Archive,
        recipe: DocumentRecipe,
    ) -> Result<HashMap<i64, ThreadSummary>, StoreError> {
        let thread_ids = self
            .rows
            .iter()
            .map(|row| row.thread_id)
            .collect::<Vec<_>>();
        let coverage_by_thread = load_thread_coverage(&archive.reader, &thread_ids).await?;
        let mut summaries = HashMap::with_capacity(self.rows.len());
        for row in &self.rows {
            let repository = serde_json::from_str(&row.repository_json)?;
            let discussion = serde_json::from_str(&row.discussion_json)?;
            let coverage = coverage_for_kind(&discussion, coverage_by_thread.get(&row.thread_id));
            if recipe == DocumentRecipe::DiscussionEnriched
                && coverage.iter().any(|coverage| coverage.is_stale())
            {
                continue;
            }
            summaries.insert(
                row.thread_id,
                ThreadSummary {
                    repository,
                    discussion,
                    coverage,
                },
            );
        }
        Ok(summaries)
    }

    /// Keeps raw-candidate cursor progress even when every selected vector set is rejected.
    fn page(
        self,
        mut summaries: HashMap<i64, ThreadSummary>,
        mut chunks: CompatibleChunks,
        limit: u32,
    ) -> EmbeddingDocumentPage {
        let next_document_id = (self.rows.len() == usize::try_from(limit).unwrap_or(usize::MAX))
            .then(|| self.rows.last().map(|row| row.document_id))
            .flatten();
        let mut items = Vec::with_capacity(self.rows.len());
        for row in self.rows {
            let Some(summary) = summaries.remove(&row.thread_id) else {
                continue;
            };
            let Some(chunks) = chunks.take(row.document_id) else {
                continue;
            };
            items.push(EmbeddingSearchDocument { summary, chunks });
        }
        EmbeddingDocumentPage {
            items,
            next_document_id,
        }
    }
}

/// Chunk groups and poisoned document IDs from one bounded vector read.
struct CompatibleChunks {
    /// Successfully decoded chunks in SQL index order, still awaiting whole-set validation.
    by_document: HashMap<i64, Vec<StoredEmbeddingChunk>>,
    /// Document IDs with any undecodable chunk; valid siblings cannot salvage those documents.
    invalid: HashSet<i64>,
}
impl CompatibleChunks {
    /// Retains corruption as a document-level rejection rather than a partial vector set.
    fn decode(rows: Vec<sqlx::sqlite::SqliteRow>) -> Result<Self, StoreError> {
        let mut chunks = Self {
            by_document: HashMap::new(),
            invalid: HashSet::new(),
        };
        for row in rows {
            let id = row.try_get("document_id")?;
            match decode_embedding_chunk(row) {
                Ok(chunk) => chunks.by_document.entry(id).or_default().push(chunk),
                Err(_) => {
                    chunks.invalid.insert(id);
                }
            }
        }
        Ok(chunks)
    }
    /// Consumes a complete valid chunk set, withholding any poisoned or incomplete document.
    fn take(&mut self, id: i64) -> Option<Vec<StoredEmbeddingChunk>> {
        let chunks = self.by_document.remove(&id)?;
        (!self.invalid.contains(&id) && complete_chunk_set(&chunks)).then_some(chunks)
    }
}
