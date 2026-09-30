//! # Select current document chunks for materialization
//!
//! `EmbeddingSelection` accounts for unique source documents and separates satisfied chunks from
//! work requiring a model request. The embedding coordinator collects this phase before acquiring
//! its writer lease: archive reads and deterministic splitting do not persist vectors.
//!
//! Duplicate identity includes thread, recipe, and full content hash. Different recipes or content
//! remain distinct even when they belong to the same thread. Replacement still reads the existing
//! chunk records, preserving archive error handling, but selects every current chunk for a request.
//! Reuse delegates positional hash and dimension checks to `chunks` after the store establishes
//! document and service identity. Empty documents count as considered documents without tasks.
//!
//! Each `EmbeddingTask` retains the full source document alongside one chunk. Later persistence
//! needs both identities; a chunk hash alone cannot establish which archived source it represents.

use std::collections::HashSet;
use std::sync::Arc;

use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_core::identity::ThreadId;
use forgesync_store::archive::Archive;
use forgesync_store::embeddings::StoredEmbeddingChunk;

use crate::embedding_client::EmbeddingClient;
use crate::embeddings::chunks::{DocumentChunk, chunk_document, compatible_chunks};
use crate::embeddings::{EmbeddingPolicy, EmbeddingReport};
use crate::error::EngineError;

/// Pending work and source accounting for one service's selected document set.
pub struct EmbeddingSelection {
    /// Selected, reusable, and document counts before any model requests begin.
    pub report: EmbeddingReport,
    /// Current inputs requiring requests, in source-document and chunk order.
    pub tasks: Vec<EmbeddingTask>,
    /// Source versions already considered during this selection, independent of chunk identity.
    seen: HashSet<(ThreadId, DocumentRecipe, String)>,
    /// Whether compatible vectors satisfy the request or every input needs replacement.
    policy: EmbeddingPolicy,
}

impl EmbeddingSelection {
    /// Reads reusable vectors and collects pending inputs without acquiring a writer lease.
    /// Invalid splitting or archive reads fail before requests or writes begin.
    pub async fn collect(
        archive: &Archive,
        client: &EmbeddingClient,
        documents: &[Document],
        policy: EmbeddingPolicy,
    ) -> Result<Self, EngineError> {
        let mut selection = Self {
            report: EmbeddingReport::default(),
            tasks: Vec::new(),
            seen: HashSet::new(),
            policy,
        };
        for document in documents {
            selection.select(archive, client, document).await?;
        }
        Ok(selection)
    }

    /// Accounts for one new source version and checks service-scoped persisted chunks.
    async fn select(
        &mut self,
        archive: &Archive,
        client: &EmbeddingClient,
        document: &Document,
    ) -> Result<(), EngineError> {
        if !self.include(document) {
            return Ok(());
        }
        let chunks = chunk_document(&document.text, client.max_input_bytes())?;
        let count = u32::try_from(chunks.len()).map_err(|_| EngineError::InvalidEmbeddingInput)?;
        self.report.chunks_selected = self.report.chunks_selected.saturating_add(chunks.len());
        if count == 0 {
            return Ok(());
        }
        let existing = archive
            .embedding_chunks(document, client.endpoint_identity(), client.model(), count)
            .await?;
        let pending = match self.policy {
            EmbeddingPolicy::Replace => chunks,
            EmbeddingPolicy::Missing => self.missing_chunks(existing, chunks, client.dimensions()),
        };
        self.add_tasks(document, pending);
        Ok(())
    }

    /// Keeps unmatched inputs and accounts for compatible vectors already satisfying the request.
    fn missing_chunks(
        &mut self,
        existing: Vec<StoredEmbeddingChunk>,
        chunks: Vec<DocumentChunk>,
        dimensions: Option<u32>,
    ) -> Vec<DocumentChunk> {
        let compatible = compatible_chunks(existing, chunks, dimensions);
        self.report.chunks_skipped = self
            .report
            .chunks_skipped
            .saturating_add(compatible.skipped);
        compatible.pending
    }

    /// Records a source version once and reports whether it needs selection.
    fn include(&mut self, document: &Document) -> bool {
        let identity = (
            document.source_identity.clone(),
            document.recipe,
            document.content_hash.clone(),
        );
        if !self.seen.insert(identity) {
            return false;
        }
        self.report.documents = self.report.documents.saturating_add(1);
        true
    }

    /// Shares one immutable full source document across its pending chunk requests.
    fn add_tasks(&mut self, document: &Document, chunks: Vec<DocumentChunk>) {
        let document = Arc::new(document.clone());
        self.tasks
            .extend(chunks.into_iter().map(|chunk| EmbeddingTask {
                document: Arc::clone(&document),
                chunk,
            }));
    }
}

/// Pending model input tied to the immutable source document used for fenced persistence.
pub struct EmbeddingTask {
    /// Shared full document identity and content hash, retained across its separate chunks.
    pub document: Arc<Document>,
    /// Deterministic position and text requested from the service.
    pub chunk: DocumentChunk,
}
