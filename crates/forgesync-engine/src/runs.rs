//! # Inspect and retry recorded workflow work
//!
//! `RetryScope`, `RetryPlan`, and `RetryReport` describe which failed jobs can be attempted again
//! and what happened on retry. List and show operations read the durable run ledger from the
//! archive.
//!
//! Retry uses recorded failure scope and current archive state rather than guessing from missing
//! content. The new attempt is a workflow with its own report; prior successes and failures remain
//! inspectable. The store owns ledger persistence, while this module decides what the engine
//! should run again. `planning` resolves failed targets, restores recorded scope, and combines
//! selected families before execution. It keeps ledger interpretation separate from acquisition;
//! `run_retry` consumes that plan through the regular fenced sync workflow.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::identity::RunId;
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use forgesync_store::runs::{RunDetail, RunRecord};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::error::EngineError;
use crate::reference::RepositorySelector;
use crate::sync::{SyncReport, SyncRequest, SyncThreadScope, sync_repositories};

mod planning;

/// One repository and scope selected by an explicit run retry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryScope {
    /// Repository whose unresolved failure is being retried.
    pub repository: RepositorySelector,
    /// Thread-state scope recorded on the failed work.
    pub scope: SyncThreadScope,
    /// Retry comments if this family had unresolved work.
    pub include_comments: bool,
    /// Retry pull-request reviews if this family had unresolved work.
    pub include_reviews: bool,
    /// Retry review threads if this family had unresolved work.
    pub include_review_threads: bool,
}

/// Exact unresolved work selected for retry before provider clients are constructed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryPlan {
    /// Run that owns the unresolved failures.
    pub parent_run_id: RunId,
    /// Failures selected for retry.
    pub failure_ids: Vec<i64>,
    /// Repository and scope requests needed to retry those failures.
    pub scopes: Vec<RetryScope>,
}

/// Results for each repository and scope executed by `run retry`.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct RetryReport {
    /// Run that owns the original failures.
    pub parent_run_id: RunId,
    /// Durable failure rows selected before retry began.
    pub failure_ids: Vec<i64>,
    /// Sync results for each selected repository and scope.
    pub runs: Vec<SyncReport>,
}

/// Lists recent durable runs in newest-first order.
pub async fn list_runs(archive: &Archive, limit: u32) -> Result<Vec<RunRecord>, EngineError> {
    archive.list_runs(limit).await.map_err(Into::into)
}

/// Loads a run and its durable jobs and failures.
pub async fn show_run(archive: &Archive, run_id: RunId) -> Result<RunDetail, EngineError> {
    archive
        .run_detail(run_id)
        .await?
        .ok_or(EngineError::RunMissing { id: run_id.get() })
}

/// Plans a retry using only unresolved failures attached to the selected run.
pub async fn plan_run_retry(
    archive: &Archive,
    run_id: RunId,
    families: &[EvidenceFamily],
) -> Result<RetryPlan, EngineError> {
    let detail = show_run(archive, run_id).await?;
    planning::plan(&detail, families)
}

/// Retries the supplied plan through the regular fenced sync operation.
pub async fn run_retry(
    archive: &Archive,
    clients: &std::collections::HashMap<forgesync_core::identity::GitHubHost, GitHubClient>,
    plan: RetryPlan,
    cancellation: &CancellationToken,
    progress: Option<mpsc::Sender<crate::sync::SyncProgress>>,
) -> Result<RetryReport, EngineError> {
    let mut runs = Vec::with_capacity(plan.scopes.len());
    for scope in &plan.scopes {
        let request = SyncRequest {
            repositories: vec![scope.repository.clone()],
            all: false,
            scope: scope.scope,
            include_comments: scope.include_comments,
            include_reviews: scope.include_reviews,
            include_review_threads: scope.include_review_threads,
            parent_run: Some(plan.parent_run_id),
        };
        runs.push(
            sync_repositories(archive, clients, &request, cancellation, progress.clone()).await?,
        );
    }
    Ok(RetryReport {
        parent_run_id: plan.parent_run_id,
        failure_ids: plan.failure_ids,
        runs,
    })
}
