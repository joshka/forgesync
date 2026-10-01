//! Materialize and embed refresh repository pages under one writer lease.
//!
//! Each discussion is materialized independently: a document failure is recorded with its
//! repository and number while the rest of the page is still embedded. A page-read failure ends
//! that repository; an embedding failure is retained while later pages and repositories continue.
//! Offset pages are separate reads, not a frozen repository snapshot.

use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_core::identity::ThreadReference;
use forgesync_store::archive::Archive;
use forgesync_store::leases::ArchiveLeaseToken;
use forgesync_store::reads::ThreadSummary;
use tokio_util::sync::CancellationToken;

use crate::clock::now_utc;
use crate::documents::build_document;
use crate::embedding_client::EmbeddingClient;
use crate::embeddings::{EmbeddingPolicy, embed_documents_fenced, lease_duration};
use crate::error::EngineError;
use crate::inspect::{
    ThreadFilters, ThreadListRequest, ThreadSort, ThreadStateFilter, list_threads, thread_detail,
};
use crate::lease::with_writer_lease;
use crate::reference::RepositorySelector;
use crate::refresh::status::{keep_first_failure, stage_failure};
use crate::refresh::{
    RefreshDocumentFailure, RefreshEmbeddingReport, RefreshStage, RefreshStageFailure,
    RefreshStageStatus,
};

/// Materializes and embeds each repository, retaining successful pages and the first failure.
///
/// Cancellation stops traversal after the current document and lets the embedding workflow
/// finalize its durable batches before returning the partial report.
pub async fn embed_repositories(
    archive: &Archive,
    repositories: &[RepositorySelector],
    client: &EmbeddingClient,
    recipe: DocumentRecipe,
    policy: EmbeddingPolicy,
    cancellation: &CancellationToken,
) -> RefreshStage<RefreshEmbeddingReport> {
    let mut stage = EmbeddingStage {
        archive,
        client,
        recipe,
        policy,
        report: RefreshEmbeddingReport::default(),
        first_failure: None,
    };
    let leased = with_writer_lease(
        archive,
        lease_duration(client),
        cancellation,
        async |lease, cancellation| {
            for repository in repositories {
                stage.collect(lease, repository, cancellation).await;
                if cancellation.is_cancelled() {
                    break;
                }
            }
            Ok(())
        },
    )
    .await;
    if let Err(error) = leased {
        keep_first_failure(&mut stage.first_failure, stage_failure(&error));
    }
    let status = embedding_status(&stage.report, stage.first_failure.as_ref());
    RefreshStage::with_report(status, stage.report, stage.first_failure)
}

struct EmbeddingStage<'a> {
    archive: &'a Archive,
    client: &'a EmbeddingClient,
    recipe: DocumentRecipe,
    policy: EmbeddingPolicy,
    report: RefreshEmbeddingReport,
    /// Earliest page-read or embedding failure; per-document failures stay in `report`.
    first_failure: Option<RefreshStageFailure>,
}

impl EmbeddingStage<'_> {
    /// Advances only from a successfully read page; a read failure ends this repository alone.
    async fn collect(
        &mut self,
        lease: &ArchiveLeaseToken,
        repository: &RepositorySelector,
        cancellation: &CancellationToken,
    ) {
        let mut offset = 0;
        loop {
            let page = match list_threads(self.archive, &repository_page(repository, offset)).await
            {
                Ok(page) => page,
                Err(error) => {
                    keep_first_failure(&mut self.first_failure, stage_failure(&error));
                    break;
                }
            };
            let documents = self
                .materialize(lease, repository, page.items, cancellation)
                .await;
            self.embed(lease, &documents, cancellation).await;
            if cancellation.is_cancelled() {
                self.report.embeddings.cancelled = true;
                break;
            }
            let Some(next) = page.next_offset else {
                break;
            };
            offset = next;
        }
    }

    /// Builds each document independently so one malformed discussion cannot discard its page.
    async fn materialize(
        &mut self,
        lease: &ArchiveLeaseToken,
        repository: &RepositorySelector,
        threads: Vec<ThreadSummary>,
        cancellation: &CancellationToken,
    ) -> Vec<Document> {
        let mut documents = Vec::with_capacity(threads.len());
        for thread in threads {
            let id = &thread.discussion.id;
            let reference = ThreadReference::new(id.repository().clone(), id.number());
            match self.materialize_one(lease, &reference).await {
                Ok(document) => {
                    documents.push(document);
                    self.report.documents_materialized += 1;
                }
                Err(error) => self.report.document_failures.push(RefreshDocumentFailure {
                    repository: repository.as_url(),
                    number: id.number().get(),
                    code: error.code(),
                    message: error.to_string(),
                }),
            }
            if cancellation.is_cancelled() {
                break;
            }
        }
        documents
    }

    async fn materialize_one(
        &self,
        lease: &ArchiveLeaseToken,
        reference: &ThreadReference,
    ) -> Result<Document, EngineError> {
        let detail = thread_detail(self.archive, reference).await?;
        let document = build_document(&detail, self.recipe);
        self.archive
            .upsert_document_fenced(lease, &document, now_utc()?)
            .await?;
        Ok(document)
    }

    /// Embeds one nonempty page, retaining completed batches even when later work fails.
    async fn embed(
        &mut self,
        lease: &ArchiveLeaseToken,
        documents: &[Document],
        cancellation: &CancellationToken,
    ) {
        if documents.is_empty() {
            return;
        }
        match embed_documents_fenced(
            self.archive,
            lease,
            self.client,
            documents,
            self.policy,
            cancellation,
        )
        .await
        {
            Ok(report) => self.report.embeddings.add(report),
            Err(error) => keep_first_failure(&mut self.first_failure, stage_failure(&error)),
        }
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
