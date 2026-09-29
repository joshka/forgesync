use std::sync::Arc;

use forgesync_core::document::DocumentRecipe;
use forgesync_core::identity::{GitHubHost, RunId};
use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::{
    ClusterListRequest, ClusterOptions, RefreshRequest, RefreshSyncOptions, RepositorySelector,
    RetryReport, RunStatus, SearchMode, SearchRequest, SyncJobStatus, SyncProgress, SyncRequest,
    SyncThreadScope, ThreadFilters, ThreadListRequest, ThreadSelector, ThreadSort,
    ThreadStateFilter, archive_status, dismiss_cluster, exclude_cluster_member,
    include_cluster_member, list_clusters, list_repositories, list_runs, list_threads,
    plan_run_retry, refresh, restore_cluster, run_retry, search_threads,
    set_canonical_cluster_member, show_cluster, show_run, show_thread, sync_repositories,
};
use forgesync_github::transport::GitHubClient;
use forgesync_store::Archive;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{self, Sender};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::app::{App, QueryMessage, RunFailureSummary};

const RUNS_TO_SCAN: u32 = 50;
const RUNS_TO_DETAIL: usize = 20;

pub(crate) enum QueryAction {
    Repositories,
    Threads {
        query: Option<String>,
        repositories: Vec<forgesync_engine::RepositorySelector>,
        offset: u64,
    },
    Detail(ThreadSelector),
    Coverage,
    Failures,
    Clusters {
        repositories: Vec<RepositorySelector>,
    },
    ClusterDetail {
        generation: u64,
        id: u64,
    },
    Sync {
        repositories: Vec<RepositorySelector>,
    },
    Refresh {
        repositories: Vec<RepositorySelector>,
    },
    Retry(RunId),
    DismissCluster {
        id: u64,
        dismissed: bool,
    },
    SetClusterMemberExcluded {
        id: u64,
        reference: ThreadSelector,
        excluded: bool,
    },
    SetCanonicalClusterMember {
        id: u64,
        reference: ThreadSelector,
    },
    CancelOperation,
}

#[derive(Default)]
pub(crate) struct QueryTasks {
    handles: Vec<JoinHandle<()>>,
    operation: Option<ActiveOperation>,
}

struct ActiveOperation {
    handle: JoinHandle<()>,
    cancellation: CancellationToken,
}

impl QueryTasks {
    fn push(&mut self, handle: JoinHandle<()>) {
        self.handles.retain(|task| !task.is_finished());
        self.handles.push(handle);
    }

    pub(crate) async fn stop(&mut self) {
        for task in self.handles.drain(..) {
            task.abort();
            let _ = task.await;
        }
        if let Some(operation) = self.operation.take() {
            operation.cancellation.cancel();
            let _ = operation.handle.await;
        }
    }
}

impl Drop for QueryTasks {
    fn drop(&mut self) {
        for task in &self.handles {
            task.abort();
        }
        if let Some(operation) = &self.operation {
            operation.cancellation.cancel();
        }
    }
}

pub(crate) fn start_query(
    action: QueryAction,
    app: &mut App,
    archive: &Arc<Archive>,
    clients: &Arc<std::collections::HashMap<GitHubHost, GitHubClient>>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
    match action {
        QueryAction::Repositories => {
            let generation = app.begin_repositories();
            let archive = Arc::clone(archive);
            let sender = sender.clone();
            tasks.push(runtime.spawn(async move {
                let result = list_repositories(&archive)
                    .await
                    .map_err(|error| error.to_string());
                let _ = sender
                    .send(QueryMessage::Repositories { generation, result })
                    .await;
            }));
        }
        QueryAction::Threads {
            query,
            repositories,
            offset,
        } => {
            let generation = app.begin_threads();
            let archive = Arc::clone(archive);
            let sender = sender.clone();
            tasks.push(runtime.spawn(async move {
                let filters = ThreadFilters {
                    repositories,
                    kind: None,
                    state: ThreadStateFilter::All,
                    sort: Some(if query.is_some() {
                        ThreadSort::Relevance
                    } else {
                        ThreadSort::Updated
                    }),
                    limit: 100,
                    offset,
                };
                let result = match query {
                    Some(query) => {
                        search_threads(
                            &archive,
                            &SearchRequest {
                                query,
                                mode: SearchMode::Keyword,
                                filters,
                                allow_keyword_fallback: false,
                            },
                        )
                        .await
                    }
                    None => list_threads(&archive, &ThreadListRequest { filters }).await,
                }
                .map(Box::new)
                .map_err(|error| error.to_string());
                let _ = sender
                    .send(QueryMessage::Threads {
                        generation,
                        offset,
                        result,
                    })
                    .await;
            }));
        }
        QueryAction::Detail(selector) => {
            let generation = app.begin_detail();
            let archive = Arc::clone(archive);
            let sender = sender.clone();
            tasks.push(runtime.spawn(async move {
                let result = show_thread(&archive, &selector)
                    .await
                    .map(Box::new)
                    .map_err(|error| error.to_string());
                let _ = sender
                    .send(QueryMessage::Detail { generation, result })
                    .await;
            }));
        }
        QueryAction::Coverage => {
            let generation = app.begin_coverage();
            let archive = Arc::clone(archive);
            let sender = sender.clone();
            tasks.push(runtime.spawn(async move {
                let result = archive_status(&archive)
                    .await
                    .map(Box::new)
                    .map_err(|error| error.to_string());
                let _ = sender
                    .send(QueryMessage::Coverage { generation, result })
                    .await;
            }));
        }
        QueryAction::Failures => {
            let generation = app.begin_failures();
            let archive = Arc::clone(archive);
            let sender = sender.clone();
            tasks.push(runtime.spawn(async move {
                let result = load_failures(&archive).await;
                let _ = sender
                    .send(QueryMessage::Failures { generation, result })
                    .await;
            }));
        }
        QueryAction::Clusters { repositories } => {
            let generation = app.begin_clusters();
            let archive = Arc::clone(archive);
            let sender = sender.clone();
            tasks.push(runtime.spawn(async move {
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
        QueryAction::ClusterDetail { generation, id } => {
            let archive = Arc::clone(archive);
            let sender = sender.clone();
            tasks.push(runtime.spawn(async move {
                let result = show_cluster(&archive, id)
                    .await
                    .map(Box::new)
                    .map_err(|error| error.to_string());
                let _ = sender
                    .send(QueryMessage::ClusterDetail { generation, result })
                    .await;
            }));
        }
        QueryAction::CancelOperation => {
            if let Some(operation) = &tasks.operation {
                operation.cancellation.cancel();
            }
        }
        action @ (QueryAction::Sync { .. }
        | QueryAction::Refresh { .. }
        | QueryAction::Retry(_)
        | QueryAction::DismissCluster { .. }
        | QueryAction::SetClusterMemberExcluded { .. }
        | QueryAction::SetCanonicalClusterMember { .. }) => {
            start_operation(action, app, archive, clients, runtime, sender, tasks);
        }
    }
}

fn start_operation(
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

async fn execute_operation(
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

    match action {
        QueryAction::Sync { repositories } => {
            let repositories = resolve_operation_repositories(archive, repositories).await?;
            let result = sync_repositories(
                archive,
                clients,
                &SyncRequest {
                    repositories,
                    all: false,
                    scope: SyncThreadScope::Default,
                    include_comments: false,
                    include_reviews: false,
                    include_review_threads: false,
                    parent_run: None,
                },
                cancellation,
                Some(progress),
            )
            .await;
            let report = match result {
                Ok(report) => report,
                Err(error) => return Err(format_engine_error(archive, error).await),
            };
            Ok(format!("Sync {}", outcome_name(&report.outcome)))
        }
        QueryAction::Refresh { repositories } => {
            let repositories = resolve_operation_repositories(archive, repositories).await?;
            let result = refresh(
                archive,
                clients,
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
                cancellation,
                Some(progress),
            )
            .await;
            let report = match result {
                Ok(report) => report,
                Err(error) => return Err(format_engine_error(archive, error).await),
            };
            Ok(format!("Refresh {}", outcome_name(&report.outcome)))
        }
        QueryAction::Retry(run_id) => {
            let plan = plan_run_retry(archive, *run_id, &[])
                .await
                .map_err(|error| error.to_string())?;
            let result = run_retry(archive, clients, plan, cancellation, Some(progress)).await;
            let report = match result {
                Ok(report) => report,
                Err(error) => return Err(format_engine_error(archive, error).await),
            };
            Ok(retry_summary(&report))
        }
        QueryAction::DismissCluster { id, dismissed } => {
            let result = if *dismissed {
                dismiss_cluster(archive, *id, "Dismissed in Forgesync TUI").await
            } else {
                restore_cluster(archive, *id).await
            };
            if let Err(error) = result {
                return Err(format_engine_error(archive, error).await);
            }
            Ok(if *dismissed {
                format!("Dismissed cluster {id}")
            } else {
                format!("Restored cluster {id}")
            })
        }
        QueryAction::SetClusterMemberExcluded {
            id,
            reference,
            excluded,
        } => {
            let result = if *excluded {
                exclude_cluster_member(archive, *id, reference, "Excluded in Forgesync TUI").await
            } else {
                include_cluster_member(archive, *id, reference).await
            };
            if let Err(error) = result {
                return Err(format_engine_error(archive, error).await);
            }
            Ok(if *excluded {
                format!("Excluded a member from cluster {id}")
            } else {
                format!("Included a member in cluster {id}")
            })
        }
        QueryAction::SetCanonicalClusterMember { id, reference } => {
            if let Err(error) = set_canonical_cluster_member(archive, *id, reference).await {
                return Err(format_engine_error(archive, error).await);
            }
            Ok(format!("Updated canonical member for cluster {id}"))
        }
        _ => Err("not a maintainer action".to_owned()),
    }
}

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

async fn format_engine_error(archive: &Archive, error: forgesync_engine::EngineError) -> String {
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

fn outcome_name(outcome: &OperationOutcome) -> &'static str {
    match outcome {
        OperationOutcome::Complete => "complete",
        OperationOutcome::Partial { .. } => "partial",
        OperationOutcome::Deferred { .. } => "deferred",
        OperationOutcome::Failed { .. } => "failed",
        OperationOutcome::Interrupted { .. } => "interrupted",
    }
}

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

async fn load_failures(archive: &Archive) -> Result<Vec<RunFailureSummary>, String> {
    let runs = list_runs(archive, RUNS_TO_SCAN)
        .await
        .map_err(|error| error.to_string())?;
    let mut summaries = Vec::new();
    for run in runs
        .into_iter()
        .filter(|run| run.status != RunStatus::Complete)
        .take(RUNS_TO_DETAIL)
    {
        let detail = match show_run(archive, run.id).await {
            Ok(detail) => detail,
            Err(error) => {
                summaries.push(RunFailureSummary {
                    id: run.id.get(),
                    status: run.status,
                    entries: vec![format!("Could not load run detail: {error}")],
                });
                continue;
            }
        };
        let mut entries: Vec<String> = detail
            .jobs
            .iter()
            .filter(|job| job.status != SyncJobStatus::Complete)
            .map(|job| {
                format!(
                    "{} / {:?}: {:?}",
                    job.repository.full_name, job.family, job.status
                )
            })
            .collect();
        entries.extend(
            detail
                .failures
                .iter()
                .filter(|failure| failure.resolved_at.is_none())
                .map(|failure| format!("{}: {}", failure.target, failure.failure.message)),
        );
        summaries.push(RunFailureSummary {
            id: detail.run.id.get(),
            status: detail.run.status,
            entries,
        });
    }
    Ok(summaries)
}
