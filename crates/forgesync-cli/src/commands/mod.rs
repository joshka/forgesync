//! Command execution and request construction.

use super::reports::*;
use super::*;

mod archive;
mod cluster;
mod embed;
mod github;
mod refresh;
mod retry;
mod run;
mod search;
mod sync;
mod thread;

use archive::archive_command;
use cluster::cluster_command;
use embed::{EmbedCommandRequest, embed_command};
use github::{github_api_base_url, github_clients_for_selectors, render_github_client_setup_error};
use refresh::{RefreshCommandRequest, refresh_command};
use retry::retry_command;
use run::run_command;
use search::{SearchCommandRequest, search_command};
use sync::sync_command;
use thread::thread_command;

pub(super) async fn dispatch(args: CliArgs, config: ForgesyncConfig) -> ExitCode {
    let Some(path) = args.archive else {
        return usage_error("--archive PATH is required for local archive commands");
    };

    match args.command {
        Command::Archive { command } => archive_command(&path, args.json, command).await,
        Command::Search {
            query,
            repositories,
            kind,
            state,
            mode,
            keyword_fallback,
            sort,
            limit,
            offset,
        } => {
            search_command(SearchCommandRequest {
                path: &path,
                json: args.json,
                service: config.embeddings,
                recipe: config.documents.recipe,
                query,
                repositories,
                kind,
                state,
                mode,
                keyword_fallback,
                sort,
                limit,
                offset,
            })
            .await
        }
        Command::Sync {
            repositories,
            all,
            state,
            with,
        } => {
            let cancellation = tokio_util::sync::CancellationToken::new();
            let interrupt_cancellation = cancellation.clone();
            let interrupt_task = tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    interrupt_cancellation.cancel();
                }
            });
            let request = SyncRequest {
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
            let result = sync_command(&path, request, args.json, args.verbose, &cancellation).await;
            interrupt_task.abort();
            result
        }
        Command::Refresh {
            repositories,
            no_sync,
            state,
            with,
            analyze,
            force,
        } => {
            let cancellation = tokio_util::sync::CancellationToken::new();
            let interrupt_cancellation = cancellation.clone();
            let interrupt_task = tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    interrupt_cancellation.cancel();
                }
            });
            let result = refresh_command(RefreshCommandRequest {
                archive_path: &path,
                repositories,
                no_sync,
                state,
                with,
                analyze,
                force,
                embedding_service: config.embeddings,
                recipe: config.documents.recipe,
                json: args.json,
                verbose: args.verbose,
                cancellation: &cancellation,
            })
            .await;
            interrupt_task.abort();
            result
        }
        Command::Embed {
            repositories,
            force,
            endpoint,
            model,
            api_key_env,
            dimensions,
            max_input_bytes,
            max_batch_input_bytes,
            batch_size,
            concurrency,
        } => {
            let cancellation = tokio_util::sync::CancellationToken::new();
            let interrupt_cancellation = cancellation.clone();
            let interrupt_task = tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    interrupt_cancellation.cancel();
                }
            });
            let mut service = config.embeddings;
            if let Some(endpoint) = endpoint {
                service.endpoint = endpoint;
            }
            if let Some(model) = model {
                service.model = model;
            }
            if let Some(api_key_env) = api_key_env {
                service.api_key_env = api_key_env;
            }
            if let Some(dimensions) = dimensions {
                service.dimensions = Some(dimensions);
            }
            if let Some(max_input_bytes) = max_input_bytes {
                service.max_input_bytes = max_input_bytes as usize;
            }
            if let Some(max_batch_input_bytes) = max_batch_input_bytes {
                service.max_batch_input_bytes = max_batch_input_bytes as usize;
            }
            if let Some(batch_size) = batch_size {
                service.batch_size = batch_size as usize;
            }
            if let Some(concurrency) = concurrency {
                service.concurrency = concurrency as usize;
            }
            let result = embed_command(EmbedCommandRequest {
                archive_path: &path,
                repositories,
                service,
                recipe: config.documents.recipe,
                force,
                json: args.json,
                verbose: args.verbose,
                cancellation: &cancellation,
            })
            .await;
            interrupt_task.abort();
            result
        }
        Command::Cluster { command } => {
            let cancellation = tokio_util::sync::CancellationToken::new();
            let interrupt_cancellation = cancellation.clone();
            let interrupt_task = tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    interrupt_cancellation.cancel();
                }
            });
            let result = cluster_command(
                &path,
                command,
                config.embeddings,
                config.documents.recipe,
                args.json,
                args.verbose,
                &cancellation,
            )
            .await;
            interrupt_task.abort();
            result
        }
        Command::Thread { command } => thread_command(&path, args.json, command).await,
        Command::Run { command } => run_command(&path, args.json, args.verbose, command).await,
        #[cfg(feature = "tui")]
        Command::Tui => {
            if args.json {
                return usage_error("--json is not supported by the interactive tui command");
            }
            if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
                return render_error(
                    false,
                    "tui",
                    "tui_requires_terminal",
                    "the tui command requires an interactive terminal",
                );
            }
            let archive = match Archive::open_read_write(&path).await {
                Ok(archive) => archive,
                Err(error) => return render_store_error(false, "tui", error),
            };
            let registered = match archive.list_repositories().await {
                Ok(registered) => registered,
                Err(error) => {
                    archive.close().await;
                    return render_store_error(false, "tui", error);
                }
            };
            let selectors = registered
                .iter()
                .map(RepositorySelector::from_repository)
                .collect::<Vec<_>>();
            let cancellation = tokio_util::sync::CancellationToken::new();
            let clients =
                match github_clients_for_selectors(&selectors, args.verbose, &cancellation).await {
                    Ok(clients) => clients,
                    Err(error) => {
                        archive.close().await;
                        return render_github_client_setup_error(false, "tui", error);
                    }
                };
            let result = forgesync_tui::run(archive, clients).await;
            match result {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => render_error(false, "tui", error.code(), &error.to_string()),
            }
        }
    }
}

pub(super) fn thread_filters(
    repositories: Vec<forgesync_engine::reference::RepositorySelector>,
    kind: Option<ThreadKindArg>,
    state: ThreadStateArg,
    sort: Option<ThreadSortArg>,
    limit: u32,
    offset: u64,
) -> ThreadFilters {
    ThreadFilters {
        repositories,
        kind: kind.map(|kind| match kind {
            ThreadKindArg::Issue => ThreadKind::Issue,
            ThreadKindArg::Pr => ThreadKind::PullRequest,
        }),
        state: match state {
            ThreadStateArg::All => ThreadStateFilter::All,
            ThreadStateArg::Open => ThreadStateFilter::Open,
            ThreadStateArg::Closed => ThreadStateFilter::Closed,
        },
        sort: sort.map(|sort| match sort {
            ThreadSortArg::Relevance => ThreadSort::Relevance,
            ThreadSortArg::Updated => ThreadSort::Updated,
            ThreadSortArg::Created => ThreadSort::Created,
        }),
        limit,
        offset,
    }
}
