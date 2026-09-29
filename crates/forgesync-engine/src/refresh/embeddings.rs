//! Refresh embeddings behavior.

use super::status::{keep_first_failure, stage_failure};
use super::{
    Archive, CancellationToken, DocumentRecipe, EmbeddingClient, EmbeddingReport,
    RefreshDocumentFailure, RefreshEmbeddingReport, RefreshStageFailure, RefreshStageStatus,
    RepositorySelector, ThreadFilters, ThreadListRequest, ThreadSelector, ThreadSort,
    ThreadStateFilter, embed_documents, list_threads, materialize_thread_document,
};

pub async fn collect_embedding_repositories(
    archive: &Archive,
    repositories: &[RepositorySelector],
    client: &EmbeddingClient,
    recipe: DocumentRecipe,
    force: bool,
    cancellation: &CancellationToken,
) -> (RefreshEmbeddingReport, Option<RefreshStageFailure>) {
    let mut result = RefreshEmbeddingReport::default();
    let mut first_failure = None;

    for repository in repositories {
        let mut offset = 0_u64;
        loop {
            let page = match list_threads(
                archive,
                &ThreadListRequest {
                    filters: ThreadFilters {
                        repositories: vec![repository.clone()],
                        kind: None,
                        state: ThreadStateFilter::All,
                        sort: Some(ThreadSort::Updated),
                        limit: 1000,
                        offset,
                    },
                },
            )
            .await
            {
                Ok(page) => page,
                Err(error) => {
                    keep_first_failure(&mut first_failure, stage_failure(&error));
                    break;
                }
            };

            let next_offset = page.next_offset;
            let mut documents = Vec::with_capacity(page.items.len());
            for thread in page.items {
                let selector = ThreadSelector::new(
                    RepositorySelector::from_repository(&thread.repository),
                    thread.discussion.id.number(),
                );
                match materialize_thread_document(archive, &selector, recipe).await {
                    Ok(built) => {
                        documents.push(built.document);
                        result.documents_materialized =
                            result.documents_materialized.saturating_add(1);
                    }
                    Err(error) => {
                        result.document_failures.push(RefreshDocumentFailure {
                            repository: repository.as_url(),
                            number: thread.discussion.id.number().get(),
                            code: error.code(),
                            message: error.to_string(),
                        });
                    }
                }
                if cancellation.is_cancelled() {
                    break;
                }
            }

            if !documents.is_empty() {
                match embed_documents(archive, client, &documents, force, cancellation).await {
                    Ok(report) => add_embedding_report(&mut result.embeddings, report),
                    Err(error) => {
                        keep_first_failure(&mut first_failure, stage_failure(&error));
                    }
                }
            }

            if cancellation.is_cancelled() {
                result.embeddings.cancelled = true;
                break;
            }
            let Some(next_offset) = next_offset else {
                break;
            };
            offset = next_offset;
        }
        if cancellation.is_cancelled() {
            break;
        }
    }

    (result, first_failure)
}

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
