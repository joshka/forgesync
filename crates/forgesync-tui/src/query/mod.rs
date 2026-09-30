//! # Dispatch explicit terminal requests
//!
//! [`requests::QueryAction`] carries keyboard and navigation intent into [`QueryDispatch::start`].
//! Read starters begin the panel generation, or use the cluster-detail generation already reserved
//! by navigation, before spawning archive-only work. Their typed
//! replies return through [`QueryMessage`]; the app's panel owners reject stale generations and
//! decide which cache or selection remains visible.
//!
//! `reads` prepares pending state and sends read results. `failures` selects bounded recent ledger
//! candidates, isolating unreadable run details. `operations` admits one writer through the app's
//! operation display, spawns its execution, and forwards progress and completion. `action` owns
//! request construction and engine mutation semantics, including provider-backed acquisition and
//! local maintainer decisions.
//!
//! [`tasks::QueryTasks`] keeps task handles and cooperative writer cancellation together. The
//! terminal event loop awaits orderly shutdown; read completion, writer status, and navigation
//! remain separate concerns. This module opens no archive and creates no runtime: callers supply
//! the already opened archive, provider clients, active runtime, and UI result channel.

use std::sync::Arc;

mod action;
mod failures;
mod operations;
mod progress;
mod reads;
pub mod requests;
pub mod tasks;
mod thread_page;

use forgesync_core::identity::GitHubHost;
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use tokio::runtime::Handle;
use tokio::sync::mpsc::Sender;

use crate::app::App;
use crate::app::messages::QueryMessage;
use crate::query::reads::ReadDispatch;
use crate::query::requests::QueryAction;
use crate::query::tasks::QueryTasks;
use crate::query::thread_page::ThreadRead;

/// Borrowed scheduling resources for one terminal dispatch batch.
///
/// The event loop constructs this from one session's archive, clients, runtime, reply channel, and
/// task registry. Each action updates pending app state before spawning work. Workers clone only
/// their shared resources; tracked handles and cancellation remain in the caller's registry.
/// Construction opens no archive, creates no runtime, and starts no task.
pub struct QueryDispatch<'a> {
    /// Already opened archive shared by local reads and explicitly selected writes.
    pub archive: &'a Arc<Archive>,
    /// Host-specific clients used only by provider-backed operations.
    pub clients: &'a Arc<std::collections::HashMap<GitHubHost, GitHubClient>>,
    /// Session runtime that executes workers without blocking terminal input.
    pub runtime: &'a Handle,
    /// Session reply channel used for generation-tagged results and advisory progress.
    pub sender: &'a Sender<QueryMessage>,
    /// Caller-owned handles and cooperative writer cancellation for orderly shutdown.
    pub tasks: &'a mut QueryTasks,
}

impl QueryDispatch<'_> {
    /// Routes one UI action to a background read, writer, or cancellation request.
    ///
    /// Panel owners allocate generations and reject stale replies. Writer admission is synchronous:
    /// a busy app refuses another operation before any worker is spawned. Results return through
    /// the session channel; this operation does not wait for provider or archive work to finish.
    pub fn start(&mut self, action: QueryAction, app: &mut App) {
        let mut reads = ReadDispatch {
            archive: self.archive,
            runtime: self.runtime,
            sender: self.sender,
            tasks: self.tasks,
        };
        match action {
            QueryAction::Repositories => reads.start_repositories(app),
            QueryAction::Threads {
                query,
                repositories,
                offset,
            } => reads.start_threads(
                ThreadRead {
                    query,
                    repositories,
                    offset,
                },
                app,
            ),
            QueryAction::Detail(selector) => reads.start_detail(selector, app),
            QueryAction::Coverage => reads.start_coverage(app),
            QueryAction::Failures => reads.start_failures(app),
            QueryAction::Clusters { repositories } => {
                reads.start_clusters(repositories, app);
            }
            QueryAction::ClusterDetail { generation, id } => {
                reads.start_cluster_detail(generation, id);
            }
            QueryAction::CancelOperation => self.tasks.cancel_operation(),
            action @ (QueryAction::Sync { .. }
            | QueryAction::Refresh { .. }
            | QueryAction::Retry(_)
            | QueryAction::DismissCluster { .. }
            | QueryAction::RestoreCluster { .. }
            | QueryAction::ExcludeClusterMember { .. }
            | QueryAction::IncludeClusterMember { .. }
            | QueryAction::SetCanonicalClusterMember { .. }) => {
                self.start_operation(action, app);
            }
        }
    }
}
