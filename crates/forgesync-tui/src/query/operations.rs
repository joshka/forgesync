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
//! result. [`crate::query::tasks::QueryTasks`] retains the writer handle and cooperative
//! cancellation token for shutdown. Dropping progress delivery on an unexpected writer exit aborts
//! its forwarding task.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::app::App;
use crate::app::messages::QueryMessage;
use crate::query::QueryDispatch;
use crate::query::action::execute_operation;
use crate::query::progress::ProgressForwarder;
use crate::query::requests::QueryAction;

impl QueryDispatch<'_> {
    /// Admits a writer and schedules its execution using this session's resources.
    ///
    /// Non-writer actions and an already busy app start no task. A successful admission clones the
    /// archive, clients, and sender into one worker; its progress forwarder drains before the
    /// terminal reply. The task registry retains cancellation and shutdown ownership.
    pub fn start_operation(&mut self, action: QueryAction, app: &mut App) {
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

        let archive = Arc::clone(self.archive);
        let clients = Arc::clone(self.clients);
        let sender = self.sender.clone();
        let cancellation = CancellationToken::new();
        let operation_cancellation = cancellation.clone();
        let handle = self.runtime.spawn(async move {
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

        self.tasks.track_operation(handle, cancellation);
    }
}
