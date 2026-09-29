//! # Load browser and triage projections
//!
//! Read starters fetch repositories, thread pages, detail, coverage, failures, clusters, and
//! cluster detail through engine/store APIs. `ThreadRead` keeps a thread selection attached to its
//! result.
//!
//! These calls are local archive reads. They return messages for `App` to apply rather than
//! drawing or mutating navigation directly, which keeps query completion order visible to the
//! state machine.

use std::sync::Arc;

use forgesync_engine::clustering::{ClusterListRequest, list_clusters, show_cluster};
use forgesync_engine::inspect::{
    ThreadFilters, ThreadListRequest, ThreadSort, ThreadStateFilter, archive_status,
    list_repositories, list_threads, show_thread,
};
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};
use forgesync_engine::search::{SearchMode, SearchRequest, search_threads};
use forgesync_store::archive::Archive;
use tokio::runtime::Handle;
use tokio::sync::mpsc::Sender;

use super::{QueryTasks, load_failures};
use crate::app::{App, QueryMessage};

/// Starts an archive-only repository read and tags its result with the current generation.
pub fn start_repositories(
    app: &mut App,
    archive: &Arc<Archive>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
    let generation = app.begin_repositories();
    let archive = Arc::clone(archive);
    let sender = sender.clone();
    tasks.push(runtime.spawn(async move {
        let result = list_repositories(&archive)
            .await
            .map_err(|error| error.to_string());
        let _ = sender
            .send(QueryMessage::Repositories { generation, result })
            .await;
    }));
}

pub struct ThreadRead {
    pub query: Option<String>,
    pub repositories: Vec<RepositorySelector>,
    pub offset: u64,
}

/// Starts a local discussion query for the selected repository and search scope.
pub fn start_threads(
    request: ThreadRead,
    app: &mut App,
    archive: &Arc<Archive>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
    let ThreadRead {
        query,
        repositories,
        offset,
    } = request;
    let generation = app.begin_threads();
    let archive = Arc::clone(archive);
    let sender = sender.clone();
    tasks.push(runtime.spawn(async move {
        let filters = ThreadFilters {
            repositories,
            kind: None,
            state: ThreadStateFilter::All,
            sort: Some(if query.is_some() {
                ThreadSort::Relevance
            } else {
                ThreadSort::Updated
            }),
            limit: 100,
            offset,
        };
        let result = match query {
            Some(query) => {
                search_threads(
                    &archive,
                    &SearchRequest {
                        query,
                        mode: SearchMode::Keyword,
                        filters,
                        allow_keyword_fallback: false,
                    },
                )
                .await
            }
            None => list_threads(&archive, &ThreadListRequest { filters }).await,
        }
        .map(Box::new)
        .map_err(|error| error.to_string());
        let _ = sender
            .send(QueryMessage::Threads {
                generation,
                offset,
                result,
            })
            .await;
    }));
}

/// Starts a local detail read for the selected discussion.
pub fn start_detail(
    selector: ThreadSelector,
    app: &mut App,
    archive: &Arc<Archive>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
    let generation = app.begin_detail();
    let archive = Arc::clone(archive);
    let sender = sender.clone();
    tasks.push(runtime.spawn(async move {
        let result = show_thread(&archive, &selector)
            .await
            .map(Box::new)
            .map_err(|error| error.to_string());
        let _ = sender
            .send(QueryMessage::Detail { generation, result })
            .await;
    }));
}

/// Starts a local archive coverage read without provider access.
pub fn start_coverage(
    app: &mut App,
    archive: &Arc<Archive>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
    let generation = app.begin_coverage();
    let archive = Arc::clone(archive);
    let sender = sender.clone();
    tasks.push(runtime.spawn(async move {
        let result = archive_status(&archive)
            .await
            .map(Box::new)
            .map_err(|error| error.to_string());
        let _ = sender
            .send(QueryMessage::Coverage { generation, result })
            .await;
    }));
}

/// Starts the local durable-run failure summary read.
pub fn start_failures(
    app: &mut App,
    archive: &Arc<Archive>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
    let generation = app.begin_failures();
    let archive = Arc::clone(archive);
    let sender = sender.clone();
    tasks.push(runtime.spawn(async move {
        let result = load_failures(&archive).await;
        let _ = sender
            .send(QueryMessage::Failures { generation, result })
            .await;
    }));
}

/// Starts a local cluster-list read for the selected repositories.
pub fn start_clusters(
    repositories: Vec<RepositorySelector>,
    app: &mut App,
    archive: &Arc<Archive>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
    let generation = app.begin_clusters();
    let archive = Arc::clone(archive);
    let sender = sender.clone();
    tasks.push(runtime.spawn(async move {
        let result = list_clusters(
            &archive,
            &ClusterListRequest {
                repositories,
                include_retired: true,
                limit: 100,
                offset: 0,
            },
        )
        .await
        .map(Box::new)
        .map_err(|error| error.to_string());
        let _ = sender
            .send(QueryMessage::Clusters { generation, result })
            .await;
    }));
}

/// Starts a local detail read for the selected cluster generation.
pub fn start_cluster_detail(
    generation: u64,
    id: u64,
    archive: &Arc<Archive>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
    let archive = Arc::clone(archive);
    let sender = sender.clone();
    tasks.push(runtime.spawn(async move {
        let result = show_cluster(&archive, id)
            .await
            .map(Box::new)
            .map_err(|error| error.to_string());
        let _ = sender
            .send(QueryMessage::ClusterDetail { generation, result })
            .await;
    }));
}
