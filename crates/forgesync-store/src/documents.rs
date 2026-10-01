//! Derived retrieval documents built from discussions.
//!
//! Documents are not provider evidence: a recipe change or newer observation can require
//! rebuilding them, and a changed content hash signals that embeddings need regeneration.

use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_core::identity::ThreadId;
use forgesync_core::timestamp::UtcTimestamp;
use serde::Serialize;
use sqlx::{Row, SqliteConnection};

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};
use crate::sql::{find_thread_row_id, thread_row_id, timestamp_from_sql};

/// Result of saving a versioned retrieval document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DocumentWrite {
    /// Stable archive-local document row ID.
    pub id: i64,
    /// Whether the source text identity changed and embeddings need regeneration.
    pub content_changed: bool,
}

impl Archive {
    /// Returns a stored document for one recipe and source discussion.
    pub async fn document(
        &self,
        source_identity: &ThreadId,
        recipe: DocumentRecipe,
    ) -> Result<Option<Document>, StoreError> {
        let Some(thread_row_id) = find_thread_row_id(&self.reader, source_identity).await? else {
            return Ok(None);
        };
        let row = sqlx::query(
            "SELECT source_identity_json, recipe, recipe_version, content_hash, title, text, dedupe_text, source_updated_at_us FROM documents WHERE thread_id = ? AND recipe = ?",
        )
        .bind(thread_row_id)
        .bind(recipe.as_str())
        .fetch_optional(&self.reader)
        .await?;
        row.map(decode_document).transpose()
    }

    /// Inserts or updates a document under the archive's active writer fence.
    ///
    /// An unchanged document keeps its row ID and build time. A source-clock-only change updates
    /// the recorded source time without reporting `content_changed`.
    pub async fn upsert_document_fenced(
        &self,
        token: &ArchiveLeaseToken,
        document: &Document,
        built_at: UtcTimestamp,
    ) -> Result<DocumentWrite, StoreError> {
        if document.recipe_version != DocumentRecipe::VERSION
            || document.content_hash != document.expected_content_hash()
            || document.content_hash.len() != 64
        {
            return Err(StoreError::InvalidDocument);
        }
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let thread_row_id = thread_row_id(&mut transaction, &document.source_identity).await?;
        let result = upsert_document(&mut transaction, document, thread_row_id, built_at).await?;
        transaction.commit().await?;
        Ok(result)
    }
}

/// Upserts the document row and reports whether its content hash changed.
async fn upsert_document(
    connection: &mut SqliteConnection,
    document: &Document,
    thread_row_id: i64,
    built_at: UtcTimestamp,
) -> Result<DocumentWrite, StoreError> {
    let source_identity_json = serde_json::to_string(&document.source_identity)?;
    let previous_hash: Option<String> =
        sqlx::query_scalar("SELECT content_hash FROM documents WHERE thread_id = ? AND recipe = ?")
            .bind(thread_row_id)
            .bind(document.recipe.as_str())
            .fetch_optional(&mut *connection)
            .await?;
    let content_changed = previous_hash.as_deref() != Some(document.content_hash.as_str());
    // An entirely unchanged row matches no update and returns no id.
    let id: Option<i64> = sqlx::query_scalar(
        "INSERT INTO documents (thread_id, recipe, recipe_version, source_identity_json, content_hash, title, text, dedupe_text, source_updated_at_us, built_at_us) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (thread_id, recipe) DO UPDATE SET recipe_version = excluded.recipe_version, source_identity_json = excluded.source_identity_json, content_hash = excluded.content_hash, title = excluded.title, text = excluded.text, dedupe_text = excluded.dedupe_text, source_updated_at_us = excluded.source_updated_at_us, built_at_us = CASE WHEN documents.content_hash <> excluded.content_hash OR documents.recipe_version <> excluded.recipe_version OR documents.source_identity_json <> excluded.source_identity_json THEN excluded.built_at_us ELSE documents.built_at_us END WHERE documents.recipe_version <> excluded.recipe_version OR documents.source_identity_json <> excluded.source_identity_json OR documents.content_hash <> excluded.content_hash OR documents.title <> excluded.title OR documents.text <> excluded.text OR documents.dedupe_text <> excluded.dedupe_text OR documents.source_updated_at_us <> excluded.source_updated_at_us RETURNING id",
    )
    .bind(thread_row_id)
    .bind(document.recipe.as_str())
    .bind(i64::from(document.recipe_version))
    .bind(&source_identity_json)
    .bind(&document.content_hash)
    .bind(&document.title)
    .bind(&document.text)
    .bind(&document.dedupe_text)
    .bind(document.source_updated_at.unix_microseconds())
    .bind(built_at.unix_microseconds())
    .fetch_optional(&mut *connection)
    .await?;
    let id = match id {
        Some(id) => id,
        None => {
            sqlx::query_scalar("SELECT id FROM documents WHERE thread_id = ? AND recipe = ?")
                .bind(thread_row_id)
                .bind(document.recipe.as_str())
                .fetch_one(&mut *connection)
                .await?
        }
    };
    Ok(DocumentWrite {
        id,
        content_changed,
    })
}

/// Converts a stored document row back to a versioned domain document.
fn decode_document(row: sqlx::sqlite::SqliteRow) -> Result<Document, StoreError> {
    let recipe_name: String = row.try_get("recipe")?;
    let recipe = parse_recipe(&recipe_name)?;
    let recipe_version: i64 = row.try_get("recipe_version")?;
    let recipe_version =
        u32::try_from(recipe_version).map_err(|_| StoreError::Corrupt("document_invalid"))?;
    let source_identity_json: String = row.try_get("source_identity_json")?;
    let source_identity = serde_json::from_str(&source_identity_json)?;
    let document = Document {
        source_identity,
        recipe,
        recipe_version,
        content_hash: row.try_get("content_hash")?,
        title: row.try_get("title")?,
        text: row.try_get("text")?,
        dedupe_text: row.try_get("dedupe_text")?,
        source_updated_at: timestamp_from_sql(row.try_get("source_updated_at_us")?)?,
    };
    if document.content_hash != document.expected_content_hash() {
        return Err(StoreError::Corrupt("document_invalid"));
    }
    Ok(document)
}

/// Rejects stored recipe labels unknown to this binary.
fn parse_recipe(value: &str) -> Result<DocumentRecipe, StoreError> {
    match value {
        "original_body" => Ok(DocumentRecipe::OriginalBody),
        "discussion_enriched" => Ok(DocumentRecipe::DiscussionEnriched),
        _ => Err(StoreError::Corrupt("document_invalid")),
    }
}
