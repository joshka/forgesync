//! Bounded semantic-search candidate pages.
//!
//! The next cursor comes from raw candidates, including those later rejected; otherwise an invalid
//! final candidate could block the caller from reaching valid documents on later pages.

use std::collections::{HashMap, HashSet};

use forgesync_core::document::DocumentRecipe;
use sqlx::{QueryBuilder, Row, Sqlite};

use super::{
    EmbeddingDocumentPage, EmbeddingDocumentQuery, EmbeddingSearchDocument, StoredEmbeddingChunk,
    complete_chunk_set, decode_embedding_chunk,
};
use crate::archive::Archive;
use crate::coverage_projection::{coverage_for_kind, load_thread_coverage};
use crate::error::StoreError;
use crate::reads::ThreadSummary;
use crate::sql::{push_bound_list, push_discussion_filters, push_repository_scope};

impl Archive {
    /// Reads current documents, then independently checks evidence and complete vector sets.
    ///
    /// The limit bounds raw candidates, so a page can hold fewer results (even none) and still
    /// carry a continuation cursor. Invalid chunk data rejects its whole document.
    pub async fn embedding_search_page(
        &self,
        query: &EmbeddingDocumentQuery<'_>,
    ) -> Result<EmbeddingDocumentPage, StoreError> {
        let rows = query.candidates().build().fetch_all(&self.reader).await?;
        let mut candidates = Vec::with_capacity(rows.len());
        for row in rows {
            let document_id: i64 = row.try_get("document_id")?;
            let thread_id: i64 = row.try_get("thread_id")?;
            candidates.push((document_id, thread_id, row));
        }
        let Some((last_document_id, _, _)) = candidates.last() else {
            return Ok(EmbeddingDocumentPage {
                items: Vec::new(),
                next_document_id: None,
            });
        };
        let next_document_id = (candidates.len()
            == usize::try_from(query.limit.get()).unwrap_or(usize::MAX))
        .then_some(*last_document_id);

        let thread_ids = candidates
            .iter()
            .map(|(_, thread_id, _)| *thread_id)
            .collect::<Vec<_>>();
        let coverage_by_thread = load_thread_coverage(&self.reader, &thread_ids).await?;
        let document_ids = candidates
            .iter()
            .map(|(document_id, _, _)| *document_id)
            .collect::<Vec<_>>();
        let mut chunks = self.compatible_chunks(query, &document_ids).await?;

        let mut items = Vec::with_capacity(candidates.len());
        for (document_id, thread_id, row) in candidates {
            let repository = serde_json::from_str(&row.try_get::<String, _>("repository_json")?)?;
            let discussion = serde_json::from_str(&row.try_get::<String, _>("discussion_json")?)?;
            let coverage = coverage_for_kind(&discussion, coverage_by_thread.get(&thread_id));
            if query.recipe == DocumentRecipe::DiscussionEnriched
                && coverage.iter().any(|coverage| coverage.is_stale())
            {
                continue;
            }
            let Some(chunks) = chunks.remove(&document_id) else {
                continue;
            };
            items.push(EmbeddingSearchDocument {
                summary: ThreadSummary {
                    repository,
                    discussion,
                    coverage,
                },
                chunks,
            });
        }
        Ok(EmbeddingDocumentPage {
            items,
            next_document_id,
        })
    }

    /// Loads complete, valid chunk sets for the candidates; any invalid chunk rejects its document.
    async fn compatible_chunks(
        &self,
        query: &EmbeddingDocumentQuery<'_>,
        document_ids: &[i64],
    ) -> Result<HashMap<i64, Vec<StoredEmbeddingChunk>>, StoreError> {
        let rows = query
            .vectors(document_ids)
            .build()
            .fetch_all(&self.reader)
            .await?;
        let mut by_document: HashMap<i64, Vec<StoredEmbeddingChunk>> = HashMap::new();
        let mut invalid = HashSet::new();
        for row in rows {
            let id = row.try_get("document_id")?;
            match decode_embedding_chunk(row) {
                Ok(chunk) => by_document.entry(id).or_default().push(chunk),
                Err(_) => {
                    invalid.insert(id);
                }
            }
        }
        by_document.retain(|id, chunks| !invalid.contains(id) && complete_chunk_set(chunks));
        Ok(by_document)
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
        statement
            .push(" ORDER BY d.id LIMIT ")
            .push_bind(i64::from(self.limit.get()));
        statement
    }

    /// Binds the service and current document identity for the selected candidates' vectors.
    fn vectors(&self, document_ids: &[i64]) -> QueryBuilder<Sqlite> {
        let mut statement = QueryBuilder::<Sqlite>::new(
            "SELECT e.document_id, e.chunk_index, e.chunk_count, e.chunk_hash, e.dimensions, e.vector_le FROM embeddings e JOIN documents d ON d.id = e.document_id WHERE e.endpoint = ",
        );
        statement
            .push_bind(self.endpoint)
            .push(" AND e.model = ")
            .push_bind(self.model)
            .push(" AND e.document_hash = d.content_hash AND d.recipe = ")
            .push_bind(self.recipe.as_str())
            .push(" AND d.recipe_version = ")
            .push_bind(i64::from(DocumentRecipe::VERSION))
            .push(" AND e.document_id IN (");
        push_bound_list(&mut statement, document_ids);
        statement.push(") ORDER BY e.document_id, e.chunk_index");
        statement
    }
}
