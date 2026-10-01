//! The single writer: admission, execution, and its terminal status.

use std::collections::HashMap;
use std::sync::Arc;

use forgesync_core::document::DocumentRecipe;
use forgesync_core::identity::GitHubHost;
use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::clustering::{
    ClusterOptions, dismiss_cluster, exclude_cluster_member, include_cluster_member,
    restore_cluster, set_canonical_cluster_member,
};
use forgesync_engine::error::EngineError;
use forgesync_engine::inspect::archive_status;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::refresh::{RefreshRequest, RefreshSyncOptions, refresh};
use forgesync_engine::runs::{RetryReport, plan_run_retry, run_retry};
use forgesync_engine::sync::{SyncProgress, SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use crate::app::App;
use crate::app::messages::QueryMessage;
use crate::query::progress::ProgressForwarder;
use crate::query::{Operation, QueryDispatch};

impl Operation {
    /// Short name shown in the footer while the operation runs.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Sync { .. } => "sync",
            Self::Refresh { .. } => "refresh",
            Self::Retry(_) => "retry",
            Self::DismissCluster { .. } => "dismiss cluster",
            Self::RestoreCluster { .. } => "restore cluster",
            Self::ExcludeClusterMember { .. } => "exclude cluster member",
            Self::IncludeClusterMember { .. } => "include cluster member",
            Self::SetCanonicalClusterMember { .. } => "set canonical member",
        }
    }
}

impl QueryDispatch<'_> {
    /// Spawns the writer unless one is already running.
    pub(super) fn start_operation(&mut self, operation: Operation, app: &mut App) {
        if !app.begin_operation(operation.label()) {
            return;
        }
        let archive = Arc::clone(self.archive);
        let clients = Arc::clone(self.clients);
        let sender = self.sender.clone();
        let cancellation = CancellationToken::new();
        let operation_cancellation = cancellation.clone();
        let handle = self.runtime.spawn(async move {
            let progress = ProgressForwarder::start(sender.clone());
            let result = execute(
                &operation,
                &archive,
                &clients,
                &operation_cancellation,
                progress.sender(),
            )
            .await;
            // Buffered progress must reach the app before the terminal result.
            progress.finish().await;
            let _ = sender.send(QueryMessage::OperationFinished(result)).await;
        });
        self.tasks.track_operation(handle, cancellation);
    }
}

/// Runs one writer action and returns its status line or a safe error.
async fn execute(
    operation: &Operation,
    archive: &Archive,
    clients: &HashMap<GitHubHost, GitHubClient>,
    cancellation: &CancellationToken,
    progress: Sender<SyncProgress>,
) -> Result<String, String> {
    let result = match operation {
        Operation::Sync { repositories } => {
            let request = SyncRequest {
                repositories: resolve_repositories(archive, repositories).await?,
                all: false,
                scope: SyncThreadScope::Default,
                include_comments: false,
                include_reviews: false,
                include_review_threads: false,
                parent_run: None,
            };
            sync_repositories(archive, clients, &request, cancellation, Some(progress))
                .await
                .map(|report| format!("Sync {}", report.outcome.as_str()))
        }
        Operation::Refresh { repositories } => {
            let request = RefreshRequest {
                repositories: resolve_repositories(archive, repositories).await?,
                sync: Some(RefreshSyncOptions {
                    scope: SyncThreadScope::Default,
                    include_comments: true,
                    include_reviews: true,
                    include_review_threads: true,
                }),
                analysis: Vec::new(),
                recipe: DocumentRecipe::DiscussionEnriched,
                embedding_identity: None,
                force_embeddings: false,
                cluster_options: ClusterOptions::default(),
            };
            refresh(
                archive,
                clients,
                None,
                &request,
                cancellation,
                Some(progress),
            )
            .await
            .map(|report| format!("Refresh {}", report.outcome.as_str()))
        }
        Operation::Retry(run_id) => match plan_run_retry(archive, *run_id, &[]).await {
            Ok(plan) => run_retry(archive, clients, plan, cancellation, Some(progress))
                .await
                .map(|report| retry_summary(&report)),
            Err(error) => Err(error),
        },
        Operation::DismissCluster { id } => {
            dismiss_cluster(archive, *id, "Dismissed in Forgesync TUI")
                .await
                .map(|()| format!("Dismissed cluster {id}"))
        }
        Operation::RestoreCluster { id } => restore_cluster(archive, *id)
            .await
            .map(|()| format!("Restored cluster {id}")),
        Operation::ExcludeClusterMember { id, reference } => {
            exclude_cluster_member(archive, *id, reference, "Excluded in Forgesync TUI")
                .await
                .map(|()| format!("Excluded a member from cluster {id}"))
        }
        Operation::IncludeClusterMember { id, reference } => {
            include_cluster_member(archive, *id, reference)
                .await
                .map(|()| format!("Included a member in cluster {id}"))
        }
        Operation::SetCanonicalClusterMember { id, reference } => {
            set_canonical_cluster_member(archive, *id, reference)
                .await
                .map(|()| format!("Updated canonical member for cluster {id}"))
        }
    };
    match result {
        Ok(status) => Ok(status),
        Err(error) => Err(engine_error_message(archive, error).await),
    }
}

/// Expands an empty scope to every registered repository before provider work starts.
async fn resolve_repositories(
    archive: &Archive,
    repositories: &[RepositorySelector],
) -> Result<Vec<RepositorySelector>, String> {
    if !repositories.is_empty() {
        return Ok(repositories.to_vec());
    }
    let registered = archive
        .list_repositories()
        .await
        .map_err(|error| error.to_string())?;
    if registered.is_empty() {
        return Err("no repositories are registered in this archive".to_owned());
    }
    Ok(registered
        .iter()
        .map(RepositorySelector::from_repository)
        .collect())
}

/// Names the lease owner and expiry when another writer holds the archive.
async fn engine_error_message(archive: &Archive, error: EngineError) -> String {
    if error.code() == "archive_lease_held"
        && let Ok(status) = archive_status(archive).await
    {
        let lease = &status.diagnostics.lease;
        let owner = lease.owner_id.as_deref().unwrap_or("another process");
        let expiry = lease
            .expires_at
            .format_rfc3339()
            .unwrap_or_else(|_| "unknown expiry".to_owned());
        return format!("archive writer lease is held by {owner} until {expiry}");
    }
    error.to_string()
}

// Core's `OperationOutcome` has no `as_str`; the CLI keeps its own copy of these labels.
/// Reports the most significant outcome across the retried runs: all complete, any
/// interrupted, all failed, all deferred, otherwise partial.
fn retry_summary(report: &RetryReport) -> String {
    let runs = &report.runs;
    let status = if runs
        .iter()
        .all(|run| run.outcome == OperationOutcome::Complete)
    {
        "complete"
    } else if runs
        .iter()
        .any(|run| matches!(run.outcome, OperationOutcome::Interrupted { .. }))
    {
        "interrupted"
    } else if runs
        .iter()
        .all(|run| matches!(run.outcome, OperationOutcome::Failed { .. }))
    {
        "failed"
    } else if runs
        .iter()
        .all(|run| matches!(run.outcome, OperationOutcome::Deferred { .. }))
    {
        "deferred"
    } else {
        "partial"
    };
    format!("Retry {status}: {} retry scope(s)", runs.len())
}
