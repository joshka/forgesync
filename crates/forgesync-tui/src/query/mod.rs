//! # Manage asynchronous archive requests
//!
//! `QueryAction` identifies a requested read or local operation. `QueryTasks` owns in-flight tasks
//! and their lifecycle, including cancellation on drop. `start_query` dispatches work and sends a
//! typed result back to the app.
//!
//! `reads` contains local inspection calls and `operations` contains actions that change local
//! archive state. The event loop stays responsive while a query runs, and the app remains the
//! owner of how results affect navigation.

use std::sync::Arc;

mod action;
mod operations;
mod reads;

use forgesync_core::identity::{GitHubHost, RunId};
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};
use forgesync_engine::runs::{list_runs, show_run};
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use forgesync_store::runs::{RunStatus, SyncJobStatus};
use operations::start_operation;
use reads::{
    ThreadRead, start_cluster_detail, start_clusters, start_coverage, start_detail, start_failures,
    start_repositories, start_threads,
};
use tokio::runtime::Handle;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::app::App;
use crate::app::failures::RunFailureSummary;
use crate::app::messages::QueryMessage;

const RUNS_TO_SCAN: u32 = 50;
const RUNS_TO_DETAIL: usize = 20;

pub enum QueryAction {
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
pub struct QueryTasks {
    handles: Vec<JoinHandle<()>>,
    operation: Option<ActiveOperation>,
}

struct ActiveOperation {
    handle: JoinHandle<()>,
    cancellation: CancellationToken,
}

impl QueryTasks {
    /// Tracks a read task after pruning handles for completed reads.
    fn push(&mut self, handle: JoinHandle<()>) {
        self.handles.retain(|task| !task.is_finished());
        self.handles.push(handle);
    }

    /// Aborts outstanding reads and requests cancellation of the active writer before shutdown.
    /// The writer is awaited so it can release its archive lease.
    pub async fn stop(&mut self) {
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
    match action {
        QueryAction::Repositories => start_repositories(app, archive, runtime, sender, tasks),
        QueryAction::Threads {
            query,
            repositories,
            offset,
        } => start_threads(
            ThreadRead {
                query,
                repositories,
                offset,
            },
            app,
            archive,
            runtime,
            sender,
            tasks,
        ),
        QueryAction::Detail(selector) => {
            start_detail(selector, app, archive, runtime, sender, tasks)
        }
        QueryAction::Coverage => start_coverage(app, archive, runtime, sender, tasks),
        QueryAction::Failures => start_failures(app, archive, runtime, sender, tasks),
        QueryAction::Clusters { repositories } => {
            start_clusters(repositories, app, archive, runtime, sender, tasks);
        }
        QueryAction::ClusterDetail { generation, id } => {
            start_cluster_detail(generation, id, archive, runtime, sender, tasks);
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

/// Loads recent non-complete runs and their unresolved failure summaries.
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
