//! # Load browser and triage projections
//!
//! Read starters fetch repositories, thread pages, detail, coverage, failures, clusters, and
//! cluster detail through engine/store APIs. [`ThreadRead`] prepares the discussion scope and
//! query; [`ThreadReply`] keeps its generation and offset attached to the resulting page.
//!
//! [`ReadDispatch`] borrows the archive, runtime, result sender, and task owner used by every read.
//! Each starter begins pending panel state before spawning, then gives the background task owned
//! archive/channel clones. A task never draws or changes navigation: its result is a typed message
//! for the app to apply. If the event loop has closed its channel, result delivery is discarded;
//! read tasks have no durable changes to roll back. Shutdown aborts outstanding reads through
//! [`QueryTasks`], while panel generations reject obsolete results that arrive during normal use.

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

/// Borrowed services needed to schedule local archive reads and return their results.
///
/// Panel state is passed only to the method that begins it. Provider clients are absent because
/// these reads cannot acquire fresh GitHub data or create semantic query embeddings. The owner
/// borrows the event loop's runtime and channel rather than installing either globally.
pub struct ReadDispatch<'a> {
    /// Opened archive shared with spawned reads; each task takes its own `Arc` clone.
    pub archive: &'a Arc<Archive>,
    /// Existing Tokio runtime on which read futures execute.
    pub runtime: &'a Handle,
    /// Bounded result channel consumed by the terminal event loop.
    pub sender: &'a Sender<QueryMessage>,
    /// Lifetime owner that retains, prunes, and aborts read tasks during shutdown.
    pub tasks: &'a mut QueryTasks,
}

impl ReadDispatch<'_> {
    /// Starts an archive-only repository read and tags its result with the current generation.
    pub fn start_repositories(&mut self, app: &mut App) {
        let generation = app.repository_picker.begin();
        let archive = Arc::clone(self.archive);
        let sender = self.sender.clone();
        self.tasks.push(self.runtime.spawn(async move {
            let result = list_repositories(&archive)
                .await
                .map_err(|error| error.to_string());
            let _ = sender
                .send(QueryMessage::Repositories { generation, result })
                .await;
        }));
    }

    /// Starts a local discussion query for the selected repository and search scope.
    pub fn start_threads(&mut self, request: ThreadRead, app: &mut App) {
        let offset = request.offset;
        let generation = app.begin_threads();
        let archive = Arc::clone(self.archive);
        let sender = self.sender.clone();
        self.tasks.push(self.runtime.spawn(async move {
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
    pub fn start_detail(&mut self, selector: ThreadSelector, app: &mut App) {
        let generation = app.detail_pane.begin();
        let archive = Arc::clone(self.archive);
        let sender = self.sender.clone();
        self.tasks.push(self.runtime.spawn(async move {
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
    pub fn start_coverage(&mut self, app: &mut App) {
        let generation = app.coverage_panel.begin();
        let archive = Arc::clone(self.archive);
        let sender = self.sender.clone();
        self.tasks.push(self.runtime.spawn(async move {
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
    pub fn start_failures(&mut self, app: &mut App) {
        let generation = app.failure_list.begin();
        let archive = Arc::clone(self.archive);
        let sender = self.sender.clone();
        self.tasks.push(self.runtime.spawn(async move {
            let result = recent_failures(&archive).await;
            let _ = sender
                .send(QueryMessage::Failures { generation, result })
                .await;
        }));
    }

    /// Starts a local cluster-list read for the selected repositories.
    pub fn start_clusters(&mut self, repositories: Vec<RepositorySelector>, app: &mut App) {
        let generation = app.cluster_list.begin();
        let archive = Arc::clone(self.archive);
        let sender = self.sender.clone();
        self.tasks.push(self.runtime.spawn(async move {
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
    pub fn start_cluster_detail(&mut self, generation: u64, id: u64) {
        let archive = Arc::clone(self.archive);
        let sender = self.sender.clone();
        self.tasks.push(self.runtime.spawn(async move {
            let result = show_cluster(&archive, id)
                .await
                .map(Box::new)
                .map_err(|error| error.to_string());
            let _ = sender
                .send(QueryMessage::ClusterDetail { generation, result })
                .await;
        }));
    }
}
