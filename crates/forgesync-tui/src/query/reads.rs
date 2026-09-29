//! Local archive reads and their response messages.

use super::*;

pub(super) fn start_repositories(
    app: &mut App,
    archive: &Arc<Archive>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
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

pub(super) struct ThreadRead {
    pub(super) query: Option<String>,
    pub(super) repositories: Vec<RepositorySelector>,
    pub(super) offset: u64,
}

pub(super) fn start_threads(
    request: ThreadRead,
    app: &mut App,
    archive: &Arc<Archive>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
    let ThreadRead {
        query,
        repositories,
        offset,
    } = request;
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

pub(super) fn start_detail(
    selector: ThreadSelector,
    app: &mut App,
    archive: &Arc<Archive>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
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

pub(super) fn start_coverage(
    app: &mut App,
    archive: &Arc<Archive>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
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

pub(super) fn start_failures(
    app: &mut App,
    archive: &Arc<Archive>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
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

pub(super) fn start_clusters(
    repositories: Vec<RepositorySelector>,
    app: &mut App,
    archive: &Arc<Archive>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
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

pub(super) fn start_cluster_detail(
    generation: u64,
    id: u64,
    archive: &Arc<Archive>,
    runtime: &Handle,
    sender: &Sender<QueryMessage>,
    tasks: &mut QueryTasks,
) {
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
