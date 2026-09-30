//! # Materialize and embed refresh repository pages
//!
//! [`embed_repositories`] traverses each selected repository's retained discussions in
//! updated-order pages, including open and closed states. A private execution owner carries the
//! archive, client, recipe, embedding policy, cancellation token, aggregate counts, and first
//! stage-level failure. This module adapts document and embedding workflows into refresh reporting;
//! those workflows own durable writes and service protocol respectively.
//!
//! Each discussion is materialized independently. A document failure records its repository,
//! number, and typed diagnostic while successful documents on the same page remain eligible for
//! embedding. A page-read failure ends that repository's traversal; an embedding failure is
//! retained while subsequent pages and repositories remain eligible unless cancellation stops
//! traversal.
//!
//! Cancellation is checked after each materialization and embedding page. It does not interrupt a
//! document operation midway here, and already stored documents or batches remain durable. Offset
//! pagination uses separate reads rather than a frozen repository snapshot.
//!
//! [`embedding_status`] gives interruption precedence, distinguishes partial progress from failure,
//! and treats a clean empty report as complete. [`add_embedding_report`] saturates aggregate counts
//! and preserves all batch failures and cancellation. Counts measure reported work, not unique
//! discussion identities or proof of complete source evidence.

use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_store::archive::Archive;
use forgesync_store::reads::ThreadSummary;
use tokio_util::sync::CancellationToken;

use crate::documents::materialize_thread_document;
use crate::embedding_client::EmbeddingClient;
use crate::embeddings::{EmbeddingPolicy, EmbeddingReport, embed_documents};
use crate::error::EngineError;
use crate::inspect::{
    ThreadFilters, ThreadListRequest, ThreadSort, ThreadStateFilter, list_threads,
};
use crate::reference::{RepositorySelector, ThreadSelector};
use crate::refresh::status::{keep_first_failure, stage_failure};
use crate::refresh::{
    RefreshDocumentFailure, RefreshEmbeddingReport, RefreshStage, RefreshStageFailure,
    RefreshStageStatus,
};

/// Materializes repositories independently, retaining successful pages and the first stage failure.
///
/// Document failures are recorded individually and do not prevent other documents on the page from
/// being embedded. Cancellation stops traversal after the current document and lets the embedding
/// workflow finalize its durable batches before returning its partial report.
pub async fn embed_repositories(
    archive: &Archive,
    repositories: &[RepositorySelector],
    client: &EmbeddingClient,
    recipe: DocumentRecipe,
    policy: EmbeddingPolicy,
    cancellation: &CancellationToken,
) -> RefreshStage<RefreshEmbeddingReport> {
    let mut work = RepositoryEmbeddings {
        archive,
        client,
        recipe,
        policy,
        cancellation,
        report: RefreshEmbeddingReport::default(),
        first_failure: None,
    };
    for repository in repositories {
        work.collect(repository).await;
        if cancellation.is_cancelled() {
            break;
        }
    }
    let status = embedding_status(&work.report, work.first_failure.as_ref());
    RefreshStage::with_report(status, work.report, work.first_failure)
}

/// One refresh stage's services and accumulated outcomes across independent repository pages.
struct RepositoryEmbeddings<'a> {
    /// Archive supplying local discussion pages and receiving documents and vectors.
    archive: &'a Archive,
    /// Vector service whose limits and identity apply to every page in this execution.
    client: &'a EmbeddingClient,
    /// Document rendering identity used consistently across independently materialized
    /// discussions.
    recipe: DocumentRecipe,
    /// Whether compatible chunks may be reused or must be requested again.
    policy: EmbeddingPolicy,
    /// Shared interruption signal checked between materializations and by provider work.
    cancellation: &'a CancellationToken,
    /// Reported work and per-document failures accumulated across pages, not unique source counts.
    report: RefreshEmbeddingReport,
    /// Earliest page-read or embedding-operation failure; per-document failures remain in
    /// `report`.
    first_failure: Option<RefreshStageFailure>,
}

impl RepositoryEmbeddings<'_> {
    /// Advances only from a successfully read page; a read failure ends this repository alone.
    async fn collect(&mut self, repository: &RepositorySelector) {
        let mut offset = 0;
        loop {
            let request = repository_page(repository, offset);
            let page = match list_threads(self.archive, &request).await {
                Ok(page) => page,
                Err(error) => {
                    self.record_failure(stage_failure(&error));
                    break;
                }
            };
            let next_offset = page.next_offset;
            let documents = self.materialize(repository, page.items).await;
            self.embed(&documents).await;
            if self.cancellation.is_cancelled() {
                self.report.embeddings.cancelled = true;
                break;
            }
            let Some(next) = next_offset else {
                break;
            };
            offset = next;
        }
    }

    /// Builds each document independently so one malformed discussion cannot discard its page.
    async fn materialize(
        &mut self,
        repository: &RepositorySelector,
        threads: Vec<ThreadSummary>,
    ) -> Vec<Document> {
        let mut documents = Vec::with_capacity(threads.len());
        for thread in threads {
            let number = thread.discussion.id.number();
            let selector = ThreadSelector::new(
                RepositorySelector::from_repository(&thread.repository),
                number,
            );
            match materialize_thread_document(self.archive, &selector, self.recipe).await {
                Ok(built) => self.record_document(&mut documents, built.document),
                Err(error) => self.record_document_failure(repository, number.get(), error),
            }
            if self.cancellation.is_cancelled() {
                break;
            }
        }
        documents
    }

    /// Accounts for a successfully materialized document before vector generation begins.
    fn record_document(&mut self, documents: &mut Vec<Document>, document: Document) {
        documents.push(document);
        self.report.documents_materialized = self.report.documents_materialized.saturating_add(1);
    }

    /// Retains the discussion identity and safe typed failure for retry guidance.
    fn record_document_failure(
        &mut self,
        repository: &RepositorySelector,
        number: u64,
        error: EngineError,
    ) {
        self.report.document_failures.push(RefreshDocumentFailure {
            repository: repository.as_url(),
            number,
            code: error.code(),
            message: error.to_string(),
        });
    }

    /// Embeds one nonempty page, retaining completed batches even when later work fails.
    async fn embed(&mut self, documents: &[Document]) {
        if documents.is_empty() {
            return;
        }
        match embed_documents(
            self.archive,
            self.client,
            documents,
            self.policy,
            self.cancellation,
        )
        .await
        {
            Ok(report) => add_embedding_report(&mut self.report.embeddings, report),
            Err(error) => self.record_failure(stage_failure(&error)),
        }
    }

    /// Keeps the earliest stage-level error while later repositories remain eligible for work.
    fn record_failure(&mut self, failure: RefreshStageFailure) {
        keep_first_failure(&mut self.first_failure, failure);
    }
}

/// Selects stable updated-order pages including both open and closed discussions.
fn repository_page(repository: &RepositorySelector, offset: u64) -> ThreadListRequest {
    ThreadListRequest {
        filters: ThreadFilters {
            repositories: vec![repository.clone()],
            kind: None,
            state: ThreadStateFilter::All,
            sort: Some(ThreadSort::Updated),
            limit: 1000,
            offset,
        },
    }
}

/// Derives stage status from document and vector failures.
pub fn embedding_status(
    report: &RefreshEmbeddingReport,
    failure: Option<&RefreshStageFailure>,
) -> RefreshStageStatus {
    if report.embeddings.cancelled
        || failure.is_some_and(|failure| failure.code == "operation_cancelled")
    {
        RefreshStageStatus::Interrupted
    } else if failure.is_some()
        || !report.document_failures.is_empty()
        || !report.embeddings.failed_batches.is_empty()
    {
        if report.documents_materialized > 0
            || report.embeddings.chunks_embedded > 0
            || report.embeddings.chunks_skipped > 0
        {
            RefreshStageStatus::Partial
        } else {
            RefreshStageStatus::Failed
        }
    } else {
        RefreshStageStatus::Complete
    }
}

/// Accumulates work across repository pages while preserving every failed batch and any
/// cancellation; counts alone would hide a partially successful embedding stage.
pub fn add_embedding_report(total: &mut EmbeddingReport, page: EmbeddingReport) {
    total.documents = total.documents.saturating_add(page.documents);
    total.chunks_selected = total.chunks_selected.saturating_add(page.chunks_selected);
    total.chunks_embedded = total.chunks_embedded.saturating_add(page.chunks_embedded);
    total.chunks_skipped = total.chunks_skipped.saturating_add(page.chunks_skipped);
    total.failed_batches.extend(page.failed_batches);
    total.cancelled |= page.cancelled;
}
