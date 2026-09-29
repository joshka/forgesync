//! Sync command handling.

use super::*;

pub(super) async fn sync_command(
    archive_path: &std::path::Path,
    request: SyncRequest,
    json: OutputMode,
    verbose: u8,
    cancellation: &tokio_util::sync::CancellationToken,
) -> ExitCode {
    let archive = match Archive::open_read_write(archive_path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(json, "sync", error),
    };
    let selectors = if request.all {
        match archive.list_repositories().await {
            Ok(registered) => registered
                .iter()
                .map(RepositorySelector::from_repository)
                .collect::<Vec<_>>(),
            Err(error) => {
                archive.close().await;
                return render_store_error(json, "sync", error);
            }
        }
    } else {
        request.repositories.clone()
    };
    let clients = match github_clients_for_selectors(&selectors, verbose, cancellation).await {
        Ok(clients) => clients,
        Err(error) => {
            archive.close().await;
            return render_github_client_setup_error(json, "sync", error);
        }
    };

    let (progress_sender, mut progress_receiver) = tokio::sync::mpsc::channel::<SyncProgress>(4);
    let progress_task = if verbose > 0 && !json.is_json() {
        Some(tokio::spawn(async move {
            while let Some(progress) = progress_receiver.recv().await {
                let repository = progress.repository.as_deref().unwrap_or("sync");
                eprintln!(
                    "forgesync: {}: {:?}, {}/{} jobs, {} threads, {} comments, {} PRs, {} reviews, {} review threads",
                    repository,
                    progress.status,
                    progress.completed_jobs,
                    progress.total_jobs,
                    progress.threads_seen,
                    progress.comments_seen,
                    progress.pull_request_metadata_seen,
                    progress.reviews_seen,
                    progress.review_threads_seen
                );
            }
        }))
    } else {
        drop(progress_receiver);
        None
    };
    let result = sync_repositories(
        &archive,
        &clients,
        &request,
        cancellation,
        Some(progress_sender),
    )
    .await;
    if let Some(progress_task) = progress_task {
        let _ = progress_task.await;
    }
    archive.close().await;
    match result {
        Ok(report) => {
            let exit_status = outcome_exit_code(&report.outcome);
            render_result(json, "sync", &report, sync_summary, exit_status)
        }
        Err(error) => render_engine_error(json, "sync", error),
    }
}

pub(super) async fn sync_from_cli(
    args: SyncArgs,
    path: &std::path::Path,
    json: OutputMode,
    verbose: u8,
) -> ExitCode {
    let SyncArgs {
        repositories,
        all,
        state,
        with,
    } = args;
    let cancellation = tokio_util::sync::CancellationToken::new();
    let interrupt_cancellation = cancellation.clone();
    let interrupt_task = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            interrupt_cancellation.cancel();
        }
    });
    let sync_request = SyncRequest {
        repositories,
        all,
        scope: match state {
            None => SyncThreadScope::Default,
            Some(SyncThreadStateArg::Open) => SyncThreadScope::Open,
            Some(SyncThreadStateArg::Closed) => SyncThreadScope::Closed,
            Some(SyncThreadStateArg::All) => SyncThreadScope::All,
        },
        include_comments: with.contains(&SyncIncludeArg::Comments),
        include_reviews: with.contains(&SyncIncludeArg::Reviews),
        include_review_threads: with.contains(&SyncIncludeArg::ReviewThreads),
        parent_run: None,
    };
    let result = sync_command(path, sync_request, json, verbose, &cancellation).await;
    interrupt_task.abort();
    result
}
