//! # Load browser and triage projections
//!
//! Read starters fetch repositories, thread pages, detail, coverage, failures, clusters, and
//! cluster detail through engine/store APIs. [`ThreadRead`] prepares the discussion scope and
//! query; [`ThreadReply`] keeps its generation and offset attached to the resulting page.
//!
//! These calls are local archive reads. They return messages for `App` to apply rather than
//! drawing or mutating navigation directly, which keeps query completion order visible to the
//! state machine.

use std::sync::Arc;

use forgesync_engine::clustering::{ClusterListRequest, list_clusters, show_cluster};
use forgesync_engine::inspect::{archive_status, list_repositories, show_thread};
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};
use forgesync_store::archive::Archive;
use tokio::runtime::Handle;
use tokio::sync::mpsc::Sender;

use crate::app::App;
use crate::app::messages::QueryMessage;
use crate::app::threads::ThreadReply;
use crate::query::failures::recent_failures;
use crate::query::tasks::QueryTasks;
use crate::query::thread_page::ThreadRead;

/// Starts an archive-only repository read and tags its result with the current generation.
pub fn start_repositories(
    app: &mut App,
    archive: &Arc<Archive>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
    let generation = app.repository_picker.begin();
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

/// Starts a local discussion query for the selected repository and search scope.
pub fn start_threads(
    request: ThreadRead,
    app: &mut App,
    archive: &Arc<Archive>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
    let offset = request.offset;
    let generation = app.begin_threads();
    let archive = Arc::clone(archive);
    let sender = sender.clone();
    tasks.push(runtime.spawn(async move {
        let result = request
            .page(&archive)
            .await
            .map(Box::new)
            .map_err(|error| error.to_string());
        let _ = sender
            .send(QueryMessage::Threads(ThreadReply {
                generation,
                offset,
                result,
            }))
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
    let generation = app.detail_pane.begin();
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
    let generation = app.coverage_panel.begin();
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
    let generation = app.failure_list.begin();
    let archive = Arc::clone(archive);
    let sender = sender.clone();
    tasks.push(runtime.spawn(async move {
        let result = recent_failures(&archive).await;
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
    let generation = app.cluster_list.begin();
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
