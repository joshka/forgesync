//! Run history and retry planning.
//!
//! Inspect prior runs and derive retry scopes from recorded failures. Retry planning is local and
//! read-only until a caller starts the selected workflow.

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
    let run_scope = &detail.run.scope;
    let original_scope = scope_from_json(run_scope.get("thread_scope"));
    let original_comments = bool_from_json(run_scope.get("include_comments"));
    let original_reviews = bool_from_json(run_scope.get("include_reviews"));
    let original_review_threads = bool_from_json(run_scope.get("include_review_threads"));
    let mut plan = RetryPlan {
        parent_run_id: run_id,
        failure_ids: Vec::new(),
        scopes: Vec::new(),
    };

    for failure in detail.failures.iter().filter(|failure| {
        failure.resolved_at.is_none()
            && (families.is_empty()
                || failure
                    .family
                    .is_some_and(|family| families.contains(&family)))
    }) {
        let matching_job = detail.jobs.iter().find(|job| {
            job.repository.full_name == failure.target
                && failure.family.is_none_or(|family| job.family == family)
                && (failure.scope_key.is_empty() || job.scope_key == failure.scope_key)
        });
        let repository = failure
            .repository
            .as_ref()
            .or_else(|| matching_job.map(|job| &job.repository))
            .map(RepositorySelector::from_repository)
            .or_else(|| failure.target.parse::<RepositorySelector>().ok())
            .ok_or_else(|| EngineError::RetryTargetInvalid {
                target: failure.target.clone(),
            })?;
        let scope = scope_from_failure_key(&failure.scope_key).unwrap_or(original_scope);
        let retry_scope = scope_for_family(
            repository,
            scope,
            failure.family,
            original_comments,
            original_reviews,
            original_review_threads,
        );
        if let Some(existing) = plan.scopes.iter_mut().find(|existing| {
            existing.repository == retry_scope.repository && existing.scope == retry_scope.scope
        }) {
            existing.include_comments |= retry_scope.include_comments;
            existing.include_reviews |= retry_scope.include_reviews;
            existing.include_review_threads |= retry_scope.include_review_threads;
        } else {
            plan.scopes.push(retry_scope);
        }
        plan.failure_ids.push(failure.id);
    }

    if plan.scopes.is_empty() {
        return Err(EngineError::NoRetryableWork { id: run_id.get() });
    }
    plan.scopes.sort_by(|left, right| {
        left.repository
            .as_url()
            .cmp(&right.repository.as_url())
            .then_with(|| scope_name(left.scope).cmp(scope_name(right.scope)))
    });
    plan.failure_ids.sort_unstable();
    Ok(plan)
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

fn scope_for_family(
    repository: RepositorySelector,
    scope: SyncThreadScope,
    family: Option<EvidenceFamily>,
    original_comments: bool,
    original_reviews: bool,
    original_review_threads: bool,
) -> RetryScope {
    let mut retry = RetryScope {
        repository,
        scope,
        include_comments: false,
        include_reviews: false,
        include_review_threads: false,
    };
    match family {
        Some(EvidenceFamily::Comments) => retry.include_comments = true,
        Some(EvidenceFamily::Reviews) => retry.include_reviews = true,
        Some(EvidenceFamily::ReviewThreads) => retry.include_review_threads = true,
        Some(EvidenceFamily::Threads | EvidenceFamily::PullRequestMetadata) => {}
        None => {
            retry.include_comments = original_comments;
            retry.include_reviews = original_reviews;
            retry.include_review_threads = original_review_threads;
        }
    }
    retry
}

fn scope_from_json(value: Option<&serde_json::Value>) -> SyncThreadScope {
    match value.and_then(serde_json::Value::as_str) {
        Some("open") => SyncThreadScope::Open,
        Some("closed") => SyncThreadScope::Closed,
        Some("all") => SyncThreadScope::All,
        _ => SyncThreadScope::Default,
    }
}

fn scope_from_failure_key(value: &str) -> Option<SyncThreadScope> {
    match value {
        "open" => Some(SyncThreadScope::Open),
        "closed" => Some(SyncThreadScope::Closed),
        "all" => Some(SyncThreadScope::All),
        _ => None,
    }
}

fn bool_from_json(value: Option<&serde_json::Value>) -> bool {
    value.and_then(serde_json::Value::as_bool).unwrap_or(false)
}

fn scope_name(scope: SyncThreadScope) -> &'static str {
    match scope {
        SyncThreadScope::Default => "default",
        SyncThreadScope::Open => "open",
        SyncThreadScope::Closed => "closed",
        SyncThreadScope::All => "all",
    }
}
