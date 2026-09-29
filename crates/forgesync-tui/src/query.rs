use std::sync::Arc;

mod operations;

use forgesync_core::document::DocumentRecipe;
use forgesync_core::identity::{GitHubHost, RunId};
use forgesync_core::outcome::OperationOutcome;
use forgesync_engine::clustering::{
    ClusterListRequest, ClusterOptions, dismiss_cluster, exclude_cluster_member,
    include_cluster_member, list_clusters, restore_cluster, set_canonical_cluster_member,
    show_cluster,
};
use forgesync_engine::inspect::{
    ThreadFilters, ThreadListRequest, ThreadSort, ThreadStateFilter, archive_status,
    list_repositories, list_threads, show_thread,
};
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};
use forgesync_engine::refresh::{RefreshRequest, RefreshSyncOptions, refresh};
use forgesync_engine::runs::{RetryReport, list_runs, plan_run_retry, run_retry, show_run};
use forgesync_engine::search::{SearchMode, SearchRequest, search_threads};
use forgesync_engine::sync::{SyncProgress, SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use forgesync_store::runs::{RunStatus, SyncJobStatus};
use operations::start_operation;
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
        repositories: Vec<forgesync_engine::reference::RepositorySelector>,
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
