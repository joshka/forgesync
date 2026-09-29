use std::sync::Arc;

use forgesync_engine::{
    RunStatus, SearchMode, SearchRequest, SyncJobStatus, ThreadFilters, ThreadListRequest,
    ThreadSelector, ThreadSort, ThreadStateFilter, archive_status, list_repositories, list_runs,
    list_threads, search_threads, show_run, show_thread,
};
use forgesync_store::Archive;
use tokio::runtime::Handle;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinHandle;

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
}

#[derive(Default)]
pub(crate) struct QueryTasks {
    handles: Vec<JoinHandle<()>>,
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
    }
}

impl Drop for QueryTasks {
    fn drop(&mut self) {
        for task in &self.handles {
            task.abort();
        }
    }
}

pub(crate) fn start_query(
    action: QueryAction,
    app: &mut App,
    archive: &Arc<Archive>,
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
