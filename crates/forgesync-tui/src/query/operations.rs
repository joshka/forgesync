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

use tokio::sync::mpsc;

use super::action::execute_operation;
use super::{
    ActiveOperation, App, Arc, Archive, CancellationToken, GitHubClient, GitHubHost, Handle,
    QueryAction, QueryMessage, QueryTasks, Sender, SyncProgress,
};

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
        QueryAction::DismissCluster {
            dismissed: true, ..
        } => "dismiss cluster",
        QueryAction::DismissCluster {
            dismissed: false, ..
        } => "restore cluster",
        QueryAction::SetClusterMemberExcluded { excluded: true, .. } => "exclude cluster member",
        QueryAction::SetClusterMemberExcluded {
            excluded: false, ..
        } => "include cluster member",
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
        let (progress_sender, mut progress_receiver) = mpsc::channel::<SyncProgress>(4);
        let progress_sender_for_action = progress_sender.clone();
        let progress_forwarder_sender = sender.clone();
        let progress_forwarder = tokio::spawn(async move {
            while let Some(progress) = progress_receiver.recv().await {
                if progress_forwarder_sender
                    .send(QueryMessage::OperationProgress {
                        generation,
                        progress,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });
        let result = execute_operation(
            &action,
            &archive,
            &clients,
            &operation_cancellation,
            progress_sender_for_action,
        )
        .await;
        drop(progress_sender);
        let _ = progress_forwarder.await;
        let _ = sender
            .send(QueryMessage::OperationFinished { generation, result })
            .await;
    });

    tasks.operation = Some(ActiveOperation {
        handle,
        cancellation,
    });
}
