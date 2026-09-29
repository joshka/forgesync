//! # Select and summarize refresh embedding work
//!
//! Embedding helpers choose repositories with relevant refreshed material, determine stage status,
//! and combine page or batch reports. They let the coordinator describe partial vector generation
//! precisely.
//!
//! The embedding client owns service protocol and `embeddings` owns materialization. This module
//! owns only their place in the refresh workflow and how their outcomes contribute to the stage
//! report.

use super::status::{keep_first_failure, stage_failure};
use super::{
    Archive, CancellationToken, DocumentRecipe, EmbeddingClient, EmbeddingReport,
    RefreshDocumentFailure, RefreshEmbeddingReport, RefreshStageFailure, RefreshStageStatus,
    RepositorySelector, ThreadFilters, ThreadListRequest, ThreadSelector, ThreadSort,
    ThreadStateFilter, embed_documents, list_threads, materialize_thread_document,
};
use crate::embeddings::EmbeddingPolicy;

/// Materializes repositories independently, retaining successful pages and the first stage failure.
///
/// Document failures are recorded individually and do not prevent other documents on the page from
/// being embedded. Cancellation stops traversal after the current document and lets the embedding
/// workflow finalize its durable batches before returning its partial report.
pub async fn collect_embedding_repositories(
    archive: &Archive,
    repositories: &[RepositorySelector],
    client: &EmbeddingClient,
    recipe: DocumentRecipe,
    policy: EmbeddingPolicy,
    cancellation: &CancellationToken,
) -> (RefreshEmbeddingReport, Option<RefreshStageFailure>) {
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
    (work.report, work.first_failure)
}

/// One refresh stage's services and accumulated outcomes across independent repository pages.
struct RepositoryEmbeddings<'a> {
    archive: &'a Archive,
    client: &'a EmbeddingClient,
    recipe: DocumentRecipe,
    policy: EmbeddingPolicy,
    cancellation: &'a CancellationToken,
    report: RefreshEmbeddingReport,
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
        threads: Vec<forgesync_store::reads::ThreadSummary>,
    ) -> Vec<forgesync_core::document::Document> {
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
    fn record_document(
        &mut self,
        documents: &mut Vec<forgesync_core::document::Document>,
        document: forgesync_core::document::Document,
    ) {
        documents.push(document);
        self.report.documents_materialized = self.report.documents_materialized.saturating_add(1);
    }

    /// Retains the discussion identity and safe typed failure for retry guidance.
    fn record_document_failure(
        &mut self,
        repository: &RepositorySelector,
        number: u64,
        error: crate::error::EngineError,
    ) {
        self.report.document_failures.push(RefreshDocumentFailure {
            repository: repository.as_url(),
            number,
            code: error.code(),
            message: error.to_string(),
        });
    }

    /// Embeds one nonempty page, retaining completed batches even when later work fails.
    async fn embed(&mut self, documents: &[forgesync_core::document::Document]) {
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
