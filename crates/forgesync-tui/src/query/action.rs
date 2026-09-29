//! # Execute a selected maintainer action
//!
//! `OperationExecution` binds an opened archive, provider clients, cancellation, and progress to
//! one action. Its dispatcher delegates to methods that construct requests and explain results.
//! Sync, refresh, and retry check the writer lease before acquisition; local cluster decisions
//! rely on the engine's write checks. Lease errors include the owner and expiry for recovery.
//!
//! `operations` owns spawning, progress forwarding, and generation tracking. This module owns
//! action semantics only. The app consumes the returned status text through a terminal message;
//! it never reads or mutates this execution context while work is running.

use tokio::sync::mpsc::Sender;

use super::{
    Archive, CancellationToken, ClusterOptions, DocumentRecipe, GitHubClient, GitHubHost,
    OperationOutcome, QueryAction, RefreshRequest, RefreshSyncOptions, RepositorySelector,
    RetryReport, SyncProgress, SyncRequest, SyncThreadScope, archive_status, dismiss_cluster,
    exclude_cluster_member, include_cluster_member, plan_run_retry, refresh, restore_cluster,
    run_retry, set_canonical_cluster_member, sync_repositories,
};

/// Runs the selected engine mutation with cancellation and a writable archive.
pub async fn execute_operation(
    action: &QueryAction,
    archive: &Archive,
    clients: &std::collections::HashMap<GitHubHost, GitHubClient>,
    cancellation: &CancellationToken,
    progress: tokio::sync::mpsc::Sender<SyncProgress>,
) -> Result<String, String> {
    if matches!(
        action,
        QueryAction::Sync { .. } | QueryAction::Refresh { .. } | QueryAction::Retry(_)
    ) && let Ok(status) = archive_status(archive).await
        && status.diagnostics.lease.held
    {
        let owner = status
            .diagnostics
            .lease
            .owner_id
            .as_deref()
            .unwrap_or("another process");
        let expiry = status
            .diagnostics
            .lease
            .expires_at
            .format_rfc3339()
            .unwrap_or_else(|_| "unknown expiry".to_owned());
        return Err(format!(
            "archive writer lease is held by {owner} until {expiry}"
        ));
    }

    let execution = OperationExecution {
        archive,
        clients,
        cancellation,
        progress,
    };
    execution.run(action).await
}

/// Services and progress channel for one cancellable writer action.
///
/// Action methods share the already opened archive and never construct a second runtime or
/// cancellation scope. The dispatcher selects a method; each method owns its request and message.
struct OperationExecution<'a> {
    archive: &'a Archive,
    clients: &'a std::collections::HashMap<GitHubHost, GitHubClient>,
    cancellation: &'a CancellationToken,
    progress: Sender<SyncProgress>,
}

impl OperationExecution<'_> {
    /// Dispatches an action without mixing its request construction with neighboring actions.
    async fn run(&self, action: &QueryAction) -> Result<String, String> {
        match action {
            QueryAction::Sync { repositories } => self.sync(repositories).await,
            QueryAction::Refresh { repositories } => self.refresh(repositories).await,
            QueryAction::Retry(run_id) => self.retry(run_id).await,
            QueryAction::DismissCluster {
                id,
                dismissed: true,
            } => self.dismiss(*id).await,
            QueryAction::DismissCluster {
                id,
                dismissed: false,
            } => self.restore(*id).await,
            QueryAction::SetClusterMemberExcluded {
                id,
                reference,
                excluded: true,
            } => self.exclude(*id, reference).await,
            QueryAction::SetClusterMemberExcluded {
                id,
                reference,
                excluded: false,
            } => self.include(*id, reference).await,
            QueryAction::SetCanonicalClusterMember { id, reference } => {
                self.canonical_member(id, reference).await
            }
            _ => Err("not a maintainer action".to_owned()),
        }
    }

    /// Executes the selected sync action and prepares its status message.
    async fn sync(&self, repositories: &[RepositorySelector]) -> Result<String, String> {
        let repositories = resolve_operation_repositories(self.archive, repositories).await?;
        let result = sync_repositories(
            self.archive,
            self.clients,
            &SyncRequest {
                repositories,
                all: false,
                scope: SyncThreadScope::Default,
                include_comments: false,
                include_reviews: false,
                include_review_threads: false,
                parent_run: None,
            },
            self.cancellation,
            Some(self.progress.clone()),
        )
        .await;
        let report = match result {
            Ok(report) => report,
            Err(error) => return Err(format_engine_error(self.archive, error).await),
        };
        Ok(format!("Sync {}", outcome_name(&report.outcome)))
    }

    /// Executes the selected refresh action and prepares its status message.
    async fn refresh(&self, repositories: &[RepositorySelector]) -> Result<String, String> {
        let repositories = resolve_operation_repositories(self.archive, repositories).await?;
        let result = refresh(
            self.archive,
            self.clients,
            None,
            &RefreshRequest {
                repositories,
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
            },
            self.cancellation,
            Some(self.progress.clone()),
        )
        .await;
        let report = match result {
            Ok(report) => report,
            Err(error) => return Err(format_engine_error(self.archive, error).await),
        };
        Ok(format!("Refresh {}", outcome_name(&report.outcome)))
    }

    /// Executes the selected retry action and prepares its status message.
    async fn retry(&self, run_id: &forgesync_core::identity::RunId) -> Result<String, String> {
        let plan = plan_run_retry(self.archive, *run_id, &[])
            .await
            .map_err(|error| error.to_string())?;
        let result = run_retry(
            self.archive,
            self.clients,
            plan,
            self.cancellation,
            Some(self.progress.clone()),
        )
        .await;
        let report = match result {
            Ok(report) => report,
            Err(error) => return Err(format_engine_error(self.archive, error).await),
        };
        Ok(retry_summary(&report))
    }

    /// Applies the local dismiss decision and reports its durable result.
    async fn dismiss(&self, id: u64) -> Result<String, String> {
        if let Err(error) = dismiss_cluster(self.archive, id, "Dismissed in Forgesync TUI").await {
            return Err(format_engine_error(self.archive, error).await);
        }
        Ok(format!("Dismissed cluster {id}"))
    }

    /// Applies the local restore decision and reports its durable result.
    async fn restore(&self, id: u64) -> Result<String, String> {
        if let Err(error) = restore_cluster(self.archive, id).await {
            return Err(format_engine_error(self.archive, error).await);
        }
        Ok(format!("Restored cluster {id}"))
    }

    /// Applies the local exclude decision and reports its durable result.
    async fn exclude(
        &self,
        id: u64,
        reference: &forgesync_engine::reference::ThreadSelector,
    ) -> Result<String, String> {
        if let Err(error) =
            exclude_cluster_member(self.archive, id, reference, "Excluded in Forgesync TUI").await
        {
            return Err(format_engine_error(self.archive, error).await);
        }
        Ok(format!("Excluded a member from cluster {id}"))
    }

    /// Applies the local include decision and reports its durable result.
    async fn include(
        &self,
        id: u64,
        reference: &forgesync_engine::reference::ThreadSelector,
    ) -> Result<String, String> {
        if let Err(error) = include_cluster_member(self.archive, id, reference).await {
            return Err(format_engine_error(self.archive, error).await);
        }
        Ok(format!("Included a member in cluster {id}"))
    }

    /// Executes the selected canonical member action and prepares its status message.
    async fn canonical_member(
        &self,
        id: &u64,
        reference: &forgesync_engine::reference::ThreadSelector,
    ) -> Result<String, String> {
        if let Err(error) = set_canonical_cluster_member(self.archive, *id, reference).await {
            return Err(format_engine_error(self.archive, error).await);
        }
        Ok(format!("Updated canonical member for cluster {id}"))
    }
}

/// Resolves the UI repository scope before provider-backed work starts.
async fn resolve_operation_repositories(
    archive: &Archive,
    repositories: &[RepositorySelector],
) -> Result<Vec<RepositorySelector>, String> {
    if repositories.is_empty() {
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
    } else {
        Ok(repositories.to_vec())
    }
}

/// Keeps a stable engine error classification in terminal status output.
async fn format_engine_error(
    archive: &Archive,
    error: forgesync_engine::error::EngineError,
) -> String {
    if error.code() == "archive_lease_held"
        && let Ok(status) = archive_status(archive).await
    {
        let owner = status
            .diagnostics
            .lease
            .owner_id
            .as_deref()
            .unwrap_or("another process");
        let expiry = status
            .diagnostics
            .lease
            .expires_at
            .format_rfc3339()
            .unwrap_or_else(|_| "unknown expiry".to_owned());
        return format!("archive writer lease is held by {owner} until {expiry}");
    }
    error.to_string()
}

/// Returns the terminal label for a structured workflow outcome.
fn outcome_name(outcome: &OperationOutcome) -> &'static str {
    match outcome {
        OperationOutcome::Complete => "complete",
        OperationOutcome::Partial { .. } => "partial",
        OperationOutcome::Deferred { .. } => "deferred",
        OperationOutcome::Failed { .. } => "failed",
        OperationOutcome::Interrupted { .. } => "interrupted",
    }
}

/// Summarizes completed and failed retry scopes for the status bar.
fn retry_summary(report: &RetryReport) -> String {
    let status = if report
        .runs
        .iter()
        .all(|run| run.outcome == OperationOutcome::Complete)
    {
        "complete"
    } else if report
        .runs
        .iter()
        .any(|run| matches!(run.outcome, OperationOutcome::Interrupted { .. }))
    {
        "interrupted"
    } else if report
        .runs
        .iter()
        .all(|run| matches!(run.outcome, OperationOutcome::Failed { .. }))
    {
        "failed"
    } else if report
        .runs
        .iter()
        .all(|run| matches!(run.outcome, OperationOutcome::Deferred { .. }))
    {
        "deferred"
    } else {
        "partial"
    };
    format!("Retry {status}: {} retry scope(s)", report.runs.len())
}
