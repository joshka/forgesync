//! Build the text used by local search.
//!
//! [`build_document`] is the pure rendering boundary. Both recipes include trimmed title/body and
//! nonempty labels. The enriched recipe adds only complete, nonstale child evidence, with
//! pull-request review families omitted for issues. Stable source timestamps/IDs determine child
//! order, and recognized bot authors are omitted from comment/review text.

use std::time::Duration;

use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_store::archive::Archive;
use forgesync_store::documents::DocumentWrite;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::clock::now_utc;
use crate::error::EngineError;
use crate::inspect::show_thread;
use crate::lease::with_writer_lease;
use crate::reference::ThreadSelector;

mod render;

pub use crate::documents::render::build_document;

/// Result of building and saving a current thread document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DocumentBuildReport {
    /// Generated deterministic input.
    pub document: Document,
    /// Persistence result and embedding invalidation signal.
    pub write: DocumentWrite,
}

/// Loads local thread detail and renders the selected recipe without mutation.
pub async fn build_thread_document(
    archive: &Archive,
    reference: &ThreadSelector,
    recipe: DocumentRecipe,
) -> Result<Document, EngineError> {
    let detail = show_thread(archive, reference).await?;
    Ok(build_document(&detail, recipe))
}

/// Renders local evidence, then persists the document under a newly acquired writer lease.
///
/// Rendering precedes the lease claim. Store persistence rechecks current source identity and
/// hash; the lease alone does not prove that rendered input is still current. An error after
/// persistence (for example a failed lease release) does not prove that no document was written.
pub async fn materialize_thread_document(
    archive: &Archive,
    reference: &ThreadSelector,
    recipe: DocumentRecipe,
) -> Result<DocumentBuildReport, EngineError> {
    let document = build_thread_document(archive, reference, recipe).await?;
    let built_at = now_utc()?;
    let write = with_writer_lease(
        archive,
        Duration::from_secs(60),
        &CancellationToken::new(),
        async |lease, _| {
            Ok(archive
                .upsert_document_fenced(lease, &document, built_at)
                .await?)
        },
    )
    .await?;
    Ok(DocumentBuildReport { document, write })
}

#[cfg(test)]
mod test_detail;
#[cfg(test)]
mod tests;
