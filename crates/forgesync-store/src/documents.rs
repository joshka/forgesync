//! # Persist search documents derived from discussions
//!
//! The engine builds a core `Document` from a thread detail; archive methods save and retrieve
//! that derived representation. `DocumentWrite` reports its archive row identity and whether its
//! content hash changed, allowing embedding workflows to decide whether regeneration is needed.
//!
//! Documents are not provider evidence. A recipe change or a newer discussion observation can
//! require rebuilding them. Keeping document storage distinct from thread observations makes that
//! invalidation and regeneration explicit.
//!
//! The private `write` module owns validation and the prepared SQL projection. This module owns
//! archive transaction/fence coordination and decoding stored documents. A source-clock-only update
//! preserves build time; unchanged writes preserve row identity. These writes do not acquire source
//! evidence or contact an embedding service.

use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_core::identity::ThreadId;
use forgesync_core::timestamp::UtcTimestamp;
use serde::Serialize;
use sqlx::Row;

use crate::archive::Archive;
use crate::error::StoreError;
use crate::leases::{ArchiveLeaseToken, require_active_archive_lease};

/// Result of saving a versioned retrieval document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DocumentWrite {
    /// Stable archive-local document row ID.
    pub id: i64,
    /// Whether the source text identity changed and embeddings need regeneration.
    pub content_changed: bool,
}

mod write;

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
    /// The source discussion must already exist. An unchanged document retains its row ID and
    /// build time. Source-clock-only changes update the recorded source time without requesting
    /// embedding regeneration; the returned `content_changed` compares content hashes.
    ///
    /// # Errors
    ///
    /// Invalid recipe versions or inconsistent content hashes are rejected before writer access.
    /// A read-only archive, lost lease, or absent source discussion prevents persistence. SQL and
    /// serialization errors leave this transaction uncommitted; success is returned after commit.
    pub async fn upsert_document_fenced(
        &self,
        token: &ArchiveLeaseToken,
        document: &Document,
        built_at: UtcTimestamp,
    ) -> Result<DocumentWrite, StoreError> {
        write::validate(document)?;
        let writer = self.writer.as_ref().ok_or(StoreError::ReadOnlyArchive)?;
        let mut transaction = writer.begin().await?;
        require_active_archive_lease(&mut transaction, token).await?;
        let Some(thread_row_id) =
            find_thread_row_id_on_connection(&mut transaction, &document.source_identity).await?
        else {
            return Err(StoreError::ThreadMissing);
        };
        let update = write::DocumentUpdate::new(document, thread_row_id, built_at)?;
        let result = update.persist(&mut transaction).await?;
        transaction.commit().await?;
        Ok(result)
    }
}

/// Resolves a normalized discussion identity before storing derived text.
async fn find_thread_row_id(
    pool: &sqlx::SqlitePool,
    identity: &ThreadId,
) -> Result<Option<i64>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT t.id FROM threads t JOIN repositories r ON r.id = t.repository_id WHERE r.host = ? AND r.provider_id = ? AND t.provider_id = ? AND t.number = ?",
    )
    .bind(identity.repository().host().as_str())
    .bind(identity.repository().provider_id().as_str())
    .bind(identity.provider_id().as_str())
    .bind(i64::try_from(identity.number().get()).map_err(|_| StoreError::IntegerOutOfRange)?)
    .fetch_optional(pool)
    .await?)
}

/// Resolves the same discussion identity inside an active write connection.
async fn find_thread_row_id_on_connection(
    connection: &mut sqlx::SqliteConnection,
    identity: &ThreadId,
) -> Result<Option<i64>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT t.id FROM threads t JOIN repositories r ON r.id = t.repository_id WHERE r.host = ? AND r.provider_id = ? AND t.provider_id = ? AND t.number = ?",
    )
    .bind(identity.repository().host().as_str())
    .bind(identity.repository().provider_id().as_str())
    .bind(identity.provider_id().as_str())
    .bind(i64::try_from(identity.number().get()).map_err(|_| StoreError::IntegerOutOfRange)?)
    .fetch_optional(&mut *connection)
    .await?)
}

/// Converts a stored document row back to a versioned domain document.
fn decode_document(row: sqlx::sqlite::SqliteRow) -> Result<Document, StoreError> {
    let recipe_name: String = row.try_get("recipe")?;
    let recipe = parse_recipe(&recipe_name)?;
    let recipe_version: i64 = row.try_get("recipe_version")?;
    let recipe_version = u32::try_from(recipe_version).map_err(|_| StoreError::InvalidDocument)?;
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
        source_updated_at: UtcTimestamp::from_unix_microseconds(
            row.try_get("source_updated_at_us")?,
        )
        .map_err(StoreError::InvalidCreatedAt)?,
    };
    if document.content_hash != document.expected_content_hash() {
        return Err(StoreError::InvalidDocument);
    }
    Ok(document)
}

/// Rejects stored recipe labels unknown to this binary.
fn parse_recipe(value: &str) -> Result<DocumentRecipe, StoreError> {
    match value {
        "original_body" => Ok(DocumentRecipe::OriginalBody),
        "discussion_enriched" => Ok(DocumentRecipe::DiscussionEnriched),
        _ => Err(StoreError::InvalidDocument),
    }
}
