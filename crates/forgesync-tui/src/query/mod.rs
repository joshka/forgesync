//! # Dispatch explicit terminal requests
//!
//! [`requests::QueryAction`] carries keyboard and navigation intent into [`start_query`]. Read
//! starters begin the appropriate panel generation before spawning archive-only work. Their typed
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
mod reads;
pub mod requests;
pub mod tasks;
mod thread_page;

use forgesync_core::identity::GitHubHost;
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use operations::start_operation;
use tokio::runtime::Handle;
use tokio::sync::mpsc::Sender;

use crate::app::App;
use crate::app::messages::QueryMessage;
use crate::query::reads::ReadDispatch;
use crate::query::requests::QueryAction;
use crate::query::tasks::QueryTasks;
use crate::query::thread_page::ThreadRead;

/// Routes a UI action to a background read or operation. Each completion returns through the
/// message channel so rendering and key handling stay responsive.
pub fn start_query(
    action: QueryAction,
    app: &mut App,
    archive: &Arc<Archive>,
    clients: &Arc<std::collections::HashMap<GitHubHost, GitHubClient>>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
    let mut reads = ReadDispatch {
        archive,
        runtime,
        sender,
        tasks,
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
        QueryAction::CancelOperation => tasks.cancel_operation(),
        action @ (QueryAction::Sync { .. }
        | QueryAction::Refresh { .. }
        | QueryAction::Retry(_)
        | QueryAction::DismissCluster { .. }
        | QueryAction::RestoreCluster { .. }
        | QueryAction::ExcludeClusterMember { .. }
        | QueryAction::IncludeClusterMember { .. }
        | QueryAction::SetCanonicalClusterMember { .. }) => {
            start_operation(action, app, archive, clients, runtime, sender, tasks);
        }
    }
}
