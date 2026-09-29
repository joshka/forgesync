//! # Run explicit TUI operations
//!
//! Operation dispatch invokes the selected action, including GitHub sync, refresh, retry, and
//! local cluster decisions, then sends progress and a terminal result back to the app. It does
//! not decide which key means which action; that belongs to `app/input`.
//!
//! Operations remain distinct from reads because they may perform network I/O, change archive
//! state, or both. The engine owns the workflow and the store validates durable writes. A
//! cancellation token and generation ID keep long-running results tied to the request that
//! started them.
//!
//! [`ProgressForwarder`] owns advisory delivery for the admitted writer. Execution releases its
//! producer when it returns; scheduling drains buffered progress before sending the terminal
//! result. [`QueryTasks`] retains the writer handle and cooperative cancellation token for
//! shutdown. Dropping progress delivery on an unexpected writer exit aborts its forwarding task.

use std::sync::Arc;

use forgesync_core::identity::GitHubHost;
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use tokio::runtime::Handle;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use crate::app::App;
use crate::app::messages::QueryMessage;
use crate::query::action::execute_operation;
use crate::query::progress::ProgressForwarder;
use crate::query::requests::QueryAction;
use crate::query::tasks::QueryTasks;

/// Starts one writer operation and reports progress through the UI message channel.
pub fn start_operation(
    action: QueryAction,
    app: &mut App,
    archive: &Arc<Archive>,
    clients: &Arc<std::collections::HashMap<GitHubHost, GitHubClient>>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
    let label = match &action {
        QueryAction::Sync { .. } => "sync",
        QueryAction::Refresh { .. } => "refresh",
        QueryAction::Retry(_) => "retry",
        QueryAction::DismissCluster { .. } => "dismiss cluster",
        QueryAction::RestoreCluster { .. } => "restore cluster",
        QueryAction::ExcludeClusterMember { .. } => "exclude cluster member",
        QueryAction::IncludeClusterMember { .. } => "include cluster member",
        QueryAction::SetCanonicalClusterMember { .. } => "set canonical member",
        _ => return,
    };
    let Some(generation) = app.begin_operation(label) else {
        return;
    };

    let archive = Arc::clone(archive);
    let clients = Arc::clone(clients);
    let sender = sender.clone();
    let cancellation = CancellationToken::new();
    let operation_cancellation = cancellation.clone();
    let handle = runtime.spawn(async move {
        let progress = ProgressForwarder::start(generation, sender.clone());
        let result = execute_operation(
            &action,
            &archive,
            &clients,
            &operation_cancellation,
            progress.sender(),
        )
        .await;
        progress.finish().await;
        let _ = sender
            .send(QueryMessage::OperationFinished { generation, result })
            .await;
    });

    tasks.track_operation(handle, cancellation);
}
