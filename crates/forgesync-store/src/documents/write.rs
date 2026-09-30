//! # Project a validated document into its durable row
//!
//! `DocumentUpdate` keeps the document, resolved thread row, build time, and serialized identity
//! together for one write. The archive validates content before opening its transaction, then
//! resolves the parent and prepares this projection after checking its writer fence.
//!
//! `persist` compares the previous content hash and upserts the current derived text. An unchanged
//! row has no `RETURNING` result, so its existing ID is read explicitly. Source-clock-only updates
//! retain the prior build time; content, recipe, or source-identity changes replace that time.
//!
//! The caller owns commit. This module performs no source acquisition, embedding generation, or
//! independent transaction. A write result becomes visible only after the archive commits it.

use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_core::timestamp::UtcTimestamp;
use sqlx::SqliteConnection;

use crate::documents::DocumentWrite;
use crate::error::StoreError;

/// Rejects unsupported recipes and content hashes inconsistent with the supplied text.
pub fn validate(document: &Document) -> Result<(), StoreError> {
    if document.recipe_version != DocumentRecipe::VERSION
        || document.content_hash != document.expected_content_hash()
        || document.content_hash.len() != 64
    {
        return Err(StoreError::InvalidDocument);
    }
    Ok(())
}

/// Prepared SQL projection shared by hash comparison, upsert, and unchanged-row lookup.
pub struct DocumentUpdate<'a> {
    /// Domain document whose recipe and content have already been validated.
    document: &'a Document,
    /// Parent thread resolved inside the caller's fenced transaction.
    thread_row_id: i64,
    /// Time used only when the derived document identity changes.
    built_at: UtcTimestamp,
    /// Serialized identity bound to the durable document row.
    source_identity_json: String,
}

impl<'a> DocumentUpdate<'a> {
    /// Serializes source identity after parent resolution, preserving error precedence.
    pub fn new(
        document: &'a Document,
        thread_row_id: i64,
        built_at: UtcTimestamp,
    ) -> Result<Self, StoreError> {
        Ok(Self {
            document,
            thread_row_id,
            built_at,
            source_identity_json: serde_json::to_string(&document.source_identity)?,
        })
    }

    /// Persists this projection and reports hash changes without committing the transaction.
    pub async fn persist(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<DocumentWrite, StoreError> {
        let previous_hash = self.previous_hash(connection).await?;
        let content_changed = previous_hash.as_deref() != Some(self.document.content_hash.as_str());
        let id = match self.upsert(connection).await? {
            Some(id) => id,
            None => self.existing_id(connection).await?,
        };
        Ok(DocumentWrite {
            id,
            content_changed,
        })
    }

    /// Reads the prior text identity to classify whether embeddings need regeneration.
    async fn previous_hash(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<Option<String>, StoreError> {
        let previous_hash: Option<String> = sqlx::query_scalar(
            "SELECT content_hash FROM documents WHERE thread_id = ? AND recipe = ?",
        )
        .bind(self.thread_row_id)
        .bind(self.document.recipe.as_str())
        .fetch_optional(connection)
        .await?;
        Ok(previous_hash)
    }

    /// Applies the explicit column mapping, returning no row for an entirely unchanged document.
    async fn upsert(&self, connection: &mut SqliteConnection) -> Result<Option<i64>, StoreError> {
        let id: Option<i64> = sqlx::query_scalar(
            "INSERT INTO documents (thread_id, recipe, recipe_version, source_identity_json, content_hash, title, text, dedupe_text, source_updated_at_us, built_at_us) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (thread_id, recipe) DO UPDATE SET recipe_version = excluded.recipe_version, source_identity_json = excluded.source_identity_json, content_hash = excluded.content_hash, title = excluded.title, text = excluded.text, dedupe_text = excluded.dedupe_text, source_updated_at_us = excluded.source_updated_at_us, built_at_us = CASE WHEN documents.content_hash <> excluded.content_hash OR documents.recipe_version <> excluded.recipe_version OR documents.source_identity_json <> excluded.source_identity_json THEN excluded.built_at_us ELSE documents.built_at_us END WHERE documents.recipe_version <> excluded.recipe_version OR documents.source_identity_json <> excluded.source_identity_json OR documents.content_hash <> excluded.content_hash OR documents.title <> excluded.title OR documents.text <> excluded.text OR documents.dedupe_text <> excluded.dedupe_text OR documents.source_updated_at_us <> excluded.source_updated_at_us RETURNING id",
        )
        .bind(self.thread_row_id)
        .bind(self.document.recipe.as_str())
        .bind(i64::from(self.document.recipe_version))
        .bind(&self.source_identity_json)
        .bind(&self.document.content_hash)
        .bind(&self.document.title)
        .bind(&self.document.text)
        .bind(&self.document.dedupe_text)
        .bind(self.document.source_updated_at.unix_microseconds())
        .bind(self.built_at.unix_microseconds())
        .fetch_optional(connection)
        .await?;
        Ok(id)
    }

    /// Resolves the row retained by an unchanged upsert without creating another document.
    async fn existing_id(&self, connection: &mut SqliteConnection) -> Result<i64, StoreError> {
        Ok(
            sqlx::query_scalar("SELECT id FROM documents WHERE thread_id = ? AND recipe = ?")
                .bind(self.thread_row_id)
                .bind(self.document.recipe.as_str())
                .fetch_one(connection)
                .await?,
        )
    }
}
