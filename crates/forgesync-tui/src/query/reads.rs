//! Local archive reads. Each read begins its panel's generation before spawning.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use forgesync_engine::clustering::{ClusterListRequest, list_clusters, show_cluster};
use forgesync_engine::error::EngineError;
use forgesync_engine::inspect::{
    ThreadFilters, ThreadListRequest, ThreadSort, ThreadStateFilter, archive_status,
    list_repositories, list_threads, show_thread,
};
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::search::{SearchMode, SearchRequest, search_threads};
use forgesync_store::archive::Archive;
use forgesync_store::reads::ThreadPage;

use crate::app::App;
use crate::app::messages::QueryMessage;
use crate::query::failures::recent_failures;
use crate::query::{QueryDispatch, Read};

/// Page size shared by discussion and cluster reads.
const READ_LIMIT: u32 = 100;

impl QueryDispatch<'_> {
    pub(super) fn start_read(&mut self, read: Read, app: &mut App) {
        let archive = Arc::clone(self.archive);
        let task: Pin<Box<dyn Future<Output = QueryMessage> + Send>> = match read {
            Read::Repositories => {
                let generation = app.repository_picker.rows.begin();
                Box::pin(async move {
                    let result = list_repositories(&archive).await.map_err(display);
                    QueryMessage::Repositories { generation, result }
                })
            }
            Read::Threads {
                query,
                repositories,
                offset,
            } => {
                let generation = app.begin_threads();
                Box::pin(async move {
                    let result = thread_page(&archive, query, repositories, offset)
                        .await
                        .map(Box::new)
                        .map_err(display);
                    QueryMessage::Threads {
                        generation,
                        offset,
                        result,
                    }
                })
            }
            Read::Detail(selector) => {
                let generation = app.detail_pane.begin();
                Box::pin(async move {
                    let result = show_thread(&archive, &selector)
                        .await
                        .map(Box::new)
                        .map_err(display);
                    QueryMessage::Detail { generation, result }
                })
            }
            Read::Coverage => {
                let generation = app.coverage.begin();
                Box::pin(async move {
                    let result = archive_status(&archive)
                        .await
                        .map(Box::new)
                        .map_err(display);
                    QueryMessage::Coverage { generation, result }
                })
            }
            Read::Failures => {
                let generation = app.failure_list.rows.begin();
                Box::pin(async move {
                    let result = recent_failures(&archive).await;
                    QueryMessage::Failures { generation, result }
                })
            }
            Read::Clusters { repositories } => {
                let generation = app.cluster_list.rows.begin();
                Box::pin(async move {
                    let request = ClusterListRequest {
                        repositories,
                        include_retired: true,
                        limit: READ_LIMIT,
                        offset: 0,
                    };
                    let result = list_clusters(&archive, &request)
                        .await
                        .map(Box::new)
                        .map_err(display);
                    QueryMessage::Clusters { generation, result }
                })
            }
            Read::ClusterDetail(id) => {
                let generation = app.cluster_detail_pane.begin(id);
                Box::pin(async move {
                    let result = show_cluster(&archive, id)
                        .await
                        .map(Box::new)
                        .map_err(display);
                    QueryMessage::ClusterDetail { generation, result }
                })
            }
        };
        let sender = self.sender.clone();
        // A closed channel means the session ended; reads have nothing to roll back.
        self.tasks.push(self.runtime.spawn(async move {
            let _ = sender.send(task.await).await;
        }));
    }
}

fn display(error: EngineError) -> String {
    error.to_string()
}

/// Reads a discussion page, ranked by local keyword relevance when a query was submitted.
async fn thread_page(
    archive: &Archive,
    query: Option<String>,
    repositories: Vec<RepositorySelector>,
    offset: u64,
) -> Result<ThreadPage, EngineError> {
    match query {
        Some(query) => {
            let request = SearchRequest {
                query,
                mode: SearchMode::Keyword,
                filters: thread_filters(repositories, offset, ThreadSort::Relevance),
                allow_keyword_fallback: false,
            };
            search_threads(archive, &request).await
        }
        None => {
            let request = ThreadListRequest {
                filters: thread_filters(repositories, offset, ThreadSort::Updated),
            };
            list_threads(archive, &request).await
        }
    }
}

fn thread_filters(
    repositories: Vec<RepositorySelector>,
    offset: u64,
    sort: ThreadSort,
) -> ThreadFilters {
    ThreadFilters {
        repositories,
        kind: None,
        state: ThreadStateFilter::All,
        sort: Some(sort),
        limit: READ_LIMIT,
        offset,
    }
}

#[cfg(test)]
mod tests {
    use forgesync_engine::inspect::{ThreadSort, ThreadStateFilter};

    use super::thread_filters;

    #[rstest::rstest]
    #[case::keyword_relevance(ThreadSort::Relevance)]
    #[case::ordinary_browsing(ThreadSort::Updated)]
    fn page_filters_preserve_scope_and_bounds(#[case] sort: ThreadSort) {
        let repositories = vec!["owner/repo".parse().expect("repository selector")];

        let filters = thread_filters(repositories.clone(), 200, sort);

        assert_eq!(filters.repositories, repositories);
        assert_eq!(filters.sort, Some(sort));
        assert_eq!(filters.kind, None);
        assert_eq!(filters.state, ThreadStateFilter::All);
        assert_eq!((filters.limit, filters.offset), (100, 200));
    }
}
