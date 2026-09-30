//! # Build the text used by local search
//!
//! `build_document` turns an offline `ThreadDetail` into a recipe-defined text document.
//! Materialization helpers read a thread and save the resulting document in the archive;
//! `DocumentBuildReport` describes the work done.
//!
//! A document is derived from acquired evidence. It can be rebuilt when the source changes or the
//! recipe changes, without refetching GitHub. Keeping text assembly here makes search input
//! reviewable independently of vector service calls and SQL persistence.
//!
//! [`build_document`] is the pure rendering boundary. Both recipes include trimmed title/body
//! and nonempty labels. The enriched recipe adds only complete, nonstale child evidence, with
//! pull-request review families omitted for issues. Stable source timestamps/IDs determine child
//! order, and recognized bot authors are omitted from comment/review text.
//!
//! [`build_thread_document`] obtains a local detail projection and renders it without writing.
//! [`materialize_thread_document`] additionally claims a bounded writer lease, persists the derived
//! document, and attempts release after the write. Rendering happens before lease acquisition;
//! the store validates document identity/recipe/hash, but does not rerender or compare source rows
//! with the earlier detail. The workflow must avoid concurrent source changes if it requires that
//! rendered input to remain current through persistence.
//!
//! [`DocumentBuildReport`] retains both the rendered input and the store's write/invalidation
//! result. Materialization does not call an embedding service or acquire provider evidence.
//! Document hashing and recipe identity belong to core; store owns document validation and
//! vector invalidation. Normalized deduplication text is derived separately from readable text.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::Archive;
use forgesync_store::documents::DocumentWrite;
use serde::Serialize;

use crate::error::EngineError;
use crate::inspect::show_thread;
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
///
/// No provider or embedding request runs. Detail assembly follows the local inspection boundary's
/// consistency contract; the result is not a reservation against later source changes.
/// Repository/thread lookup and store decoding errors are propagated from [`show_thread`].
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
/// The read/render phase precedes a 60-second lease claim. Store persistence rechecks current
/// source identity and hash; the lease alone does not prove that rendered input is still current.
/// Release is attempted after either write success or failure. A write error takes precedence over
/// a simultaneous release error; after successful persistence, release failure is returned even
/// though the document write is already durable. No heartbeat or embedding request is started.
///
/// # Errors
///
/// Propagates local lookup, clock, lease, persistence, and release failures. A successful report
/// includes the store's write and vector-invalidation result. An error after persistence must not
/// be interpreted as proof that no document was written.
pub async fn materialize_thread_document(
    archive: &Archive,
    reference: &ThreadSelector,
    recipe: DocumentRecipe,
) -> Result<DocumentBuildReport, EngineError> {
    let document = build_thread_document(archive, reference, recipe).await?;
    let built_at = now_utc()?;
    let lease = archive
        .acquire_archive_lease(built_at, Duration::from_secs(60))
        .await?;
    let write = archive
        .upsert_document_fenced(&lease, &document, built_at)
        .await;
    let release = archive.release_archive_lease(&lease, built_at).await;
    let write = match write {
        Ok(write) => write,
        Err(error) => return Err(error.into()),
    };
    release?;
    Ok(DocumentBuildReport { document, write })
}

/// Produces the materialization timestamp after validating the system clock.
pub(crate) fn now_utc() -> Result<UtcTimestamp, EngineError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| forgesync_store::error::StoreError::ClockOutOfRange)?;
    let micros = i64::try_from(elapsed.as_micros())
        .map_err(|_| forgesync_store::error::StoreError::ClockOutOfRange)?;
    UtcTimestamp::from_unix_microseconds(micros)
        .map_err(forgesync_store::error::StoreError::InvalidCreatedAt)
        .map_err(Into::into)
}

#[cfg(test)]
mod tests;
