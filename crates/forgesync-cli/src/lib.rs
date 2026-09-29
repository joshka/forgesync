#![forbid(unsafe_code)]

//! Process interface for the Forgesync application.

pub mod args;
pub mod config;
pub mod credentials;
pub mod output;

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io::Write;
use std::process::ExitCode;

use args::{
    ArchiveCommand, CliArgs, ClusterCommand, Command, RefreshAnalysisArg, RunCommand, RunFamilyArg,
    SearchModeArg, SyncIncludeArg, SyncThreadStateArg, ThreadCommand, ThreadKindArg, ThreadSortArg,
    ThreadStateArg,
};
use clap::{CommandFactory, Parser, error::ErrorKind};
use config::ForgesyncConfig;
use forgesync_core::{
    CoverageState, DocumentRecipe, GitHubHost, OperationOutcome, ReviewState, RunId, SourceState,
    ThreadKind, ThreadKind as DiscussionKind, UtcTimestamp,
};
use forgesync_engine::{
    ClusterBuildReport, ClusterBuildRequest, ClusterDetail, ClusterListRequest, ClusterOptions,
    ClusterPage, EmbeddingClient, EmbeddingReport, EmbeddingServiceIdentity, EngineError,
    RefreshAnalysisStage, RefreshDocumentFailure, RefreshReport, RefreshRequest,
    RefreshStageFailure, RefreshStageKind, RefreshStageStatus, RefreshSyncOptions,
    RepositorySelector, RetryReport, SearchMode, SearchRequest, SearchResultPage, SyncProgress,
    SyncReport, SyncRequest, SyncThreadScope, ThreadDetail, ThreadFilters, ThreadListRequest,
    ThreadPage, ThreadSort, ThreadStateFilter, archive_status, build_clusters, dismiss_cluster,
    embed_repositories, exclude_cluster_member, include_cluster_member, list_clusters, list_runs,
    list_threads, plan_run_retry, refresh, restore_cluster, retrieve_threads, run_retry,
    set_canonical_cluster_member, show_cluster, show_run, show_thread, sync_repositories,
};
use forgesync_github::{GitHubClient, GitHubClientConfig};
use forgesync_store::{
    Archive, ArchiveInfo, DoctorReport, MigrationReport, RunDetail, RunRecord, RunStatus,
    StoreError, SyncJobStatus, ThreadTimelineEvent,
};
use serde::Serialize;

use crate::output::{
    ArchiveStatusOutput, JsonEnvelope, SearchPageOutput, ThreadDetailOutput, ThreadPageOutput,
};

/// Parses arguments, runs the selected command, and writes its process output.
pub fn run_from<I, T>(arguments: I) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let args = match CliArgs::try_parse_from(arguments) {
        Ok(args) => args,
        Err(error) => {
            let code = error.exit_code();
            let _ = error.print();
            return ExitCode::from(u8::try_from(code).unwrap_or(2));
        }
    };

    let config = match ForgesyncConfig::load(args.config.as_deref()) {
        Ok(config) => config,
        Err(error) => {
            return render_error_with_status(
                args.json,
                "configuration",
                error.code(),
                &error.to_string(),
                ExitCode::from(2),
            );
        }
    };

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            return render_error(
                args.json,
                "startup",
                "runtime_unavailable",
                &format!("could not start async runtime: {error}"),
            );
        }
    };
    runtime.block_on(dispatch(args, config))
}

async fn dispatch(args: CliArgs, config: ForgesyncConfig) -> ExitCode {
    let Some(path) = args.archive else {
        return usage_error("--archive PATH is required for local archive commands");
    };

    match args.command {
        Command::Archive { command } => match command {
            ArchiveCommand::Init => match Archive::create(&path).await {
                Ok(archive) => {
                    let info = archive.info().clone();
                    archive.close().await;
                    render_success(args.json, "archive init", &info, archive_summary)
                }
                Err(error) => render_store_error(args.json, "archive init", error),
            },
            ArchiveCommand::Migrate => match Archive::migrate(&path).await {
                Ok(migration) => match Archive::open_read_only(&path).await {
                    Ok(archive) => {
                        let data = MigrationOutput {
                            migration,
                            archive: archive.info().clone(),
                        };
                        archive.close().await;
                        render_success(args.json, "archive migrate", &data, migration_summary)
                    }
                    Err(error) => render_store_error(args.json, "archive migrate", error),
                },
                Err(error) => render_store_error(args.json, "archive migrate", error),
            },
            ArchiveCommand::Status => match Archive::open_read_only(&path).await {
                Ok(archive) => {
                    let result = archive_status(&archive).await;
                    archive.close().await;
                    match result {
                        Ok(status) => {
                            let output = ArchiveStatusOutput::from(&status);
                            render_success(
                                args.json,
                                "archive status",
                                &output,
                                archive_status_summary,
                            )
                        }
                        Err(error) => render_engine_error(args.json, "archive status", error),
                    }
                }
                Err(error) => render_store_error(args.json, "archive status", error),
            },
            ArchiveCommand::Doctor => match Archive::open_read_only(&path).await {
                Ok(archive) => {
                    let report = match archive.doctor().await {
                        Ok(report) => report,
                        Err(error) => {
                            archive.close().await;
                            return render_store_error(args.json, "archive doctor", error);
                        }
                    };
                    archive.close().await;
                    let exit_status = if report.healthy {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    };
                    render_result(
                        args.json,
                        "archive doctor",
                        &report,
                        doctor_summary,
                        exit_status,
                    )
                }
                Err(error) => render_store_error(args.json, "archive doctor", error),
            },
        },
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
            let mode = match mode {
                SearchModeArg::Keyword => SearchMode::Keyword,
                SearchModeArg::AdvancedFts => SearchMode::AdvancedFts,
                SearchModeArg::Semantic => SearchMode::Semantic,
                SearchModeArg::Hybrid => SearchMode::Hybrid,
            };
            if keyword_fallback && !matches!(mode, SearchMode::Semantic | SearchMode::Hybrid) {
                return render_error_with_status(
                    args.json,
                    "search",
                    "search_fallback_mode_invalid",
                    "--keyword-fallback requires --mode semantic or --mode hybrid",
                    ExitCode::from(2),
                );
            }
            let embedding_client = if matches!(mode, SearchMode::Semantic | SearchMode::Hybrid) {
                let service = config.embeddings.clone();
                let api_key = std::env::var(&service.api_key_env).unwrap_or_default();
                let client_config = match service.client_config(api_key) {
                    Ok(config) => config,
                    Err(error) => {
                        return render_error_with_status(
                            args.json,
                            "search",
                            error.code(),
                            &error.to_string(),
                            ExitCode::from(2),
                        );
                    }
                };
                match EmbeddingClient::new(client_config) {
                    Ok(client) => Some(client),
                    Err(error) => {
                        return render_error(args.json, "search", error.code(), &error.to_string());
                    }
                }
            } else {
                None
            };
            let cancellation = tokio_util::sync::CancellationToken::new();
            let interrupt_cancellation = cancellation.clone();
            let interrupt_task = tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    interrupt_cancellation.cancel();
                }
            });
            let result = match Archive::open_read_only(&path).await {
                Ok(archive) => {
                    let request = SearchRequest {
                        query,
                        mode,
                        filters: thread_filters(repositories, kind, state, sort, limit, offset),
                        allow_keyword_fallback: keyword_fallback,
                    };
                    let result = retrieve_threads(
                        &archive,
                        &request,
                        config.documents.recipe,
                        embedding_client.as_ref(),
                        &cancellation,
                    )
                    .await;
                    archive.close().await;
                    match result {
                        Ok(page) => render_search_page(args.json, &page),
                        Err(error) => render_engine_error(args.json, "search", error),
                    }
                }
                Err(error) => render_store_error(args.json, "search", error),
            };
            interrupt_task.abort();
            result
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
        Command::Thread { command } => match command {
            ThreadCommand::List {
                repositories,
                kind,
                state,
                sort,
                limit,
                offset,
            } => match Archive::open_read_only(&path).await {
                Ok(archive) => {
                    let request = ThreadListRequest {
                        filters: thread_filters(repositories, kind, state, sort, limit, offset),
                    };
                    let result = list_threads(&archive, &request).await;
                    archive.close().await;
                    match result {
                        Ok(page) => render_thread_page(args.json, "thread list", &page),
                        Err(error) => render_engine_error(args.json, "thread list", error),
                    }
                }
                Err(error) => render_store_error(args.json, "thread list", error),
            },
            ThreadCommand::Show { reference } => match Archive::open_read_only(&path).await {
                Ok(archive) => {
                    let result = show_thread(&archive, &reference).await;
                    archive.close().await;
                    match result {
                        Ok(detail) => render_thread_detail(args.json, &detail),
                        Err(error) => render_engine_error(args.json, "thread show", error),
                    }
                }
                Err(error) => render_store_error(args.json, "thread show", error),
            },
        },
        Command::Run { command } => match command {
            RunCommand::List { limit } => match Archive::open_read_only(&path).await {
                Ok(archive) => {
                    let result = list_runs(&archive, limit).await;
                    archive.close().await;
                    match result {
                        Ok(runs) => render_success(args.json, "run list", &runs, run_list_summary),
                        Err(error) => render_engine_error(args.json, "run list", error),
                    }
                }
                Err(error) => render_store_error(args.json, "run list", error),
            },
            RunCommand::Show { id } => {
                let id = match RunId::new(id) {
                    Ok(id) => id,
                    Err(_) => return usage_error("run ID must be a positive integer"),
                };
                match Archive::open_read_only(&path).await {
                    Ok(archive) => {
                        let result = show_run(&archive, id).await;
                        archive.close().await;
                        match result {
                            Ok(detail) => {
                                render_success(args.json, "run show", &detail, run_detail_summary)
                            }
                            Err(error) => render_engine_error(args.json, "run show", error),
                        }
                    }
                    Err(error) => render_store_error(args.json, "run show", error),
                }
            }
            RunCommand::Retry { id, family } => {
                let id = match RunId::new(id) {
                    Ok(id) => id,
                    Err(_) => return usage_error("run ID must be a positive integer"),
                };
                let cancellation = tokio_util::sync::CancellationToken::new();
                let interrupt_cancellation = cancellation.clone();
                let interrupt_task = tokio::spawn(async move {
                    if tokio::signal::ctrl_c().await.is_ok() {
                        interrupt_cancellation.cancel();
                    }
                });
                let result = retry_command(
                    &path,
                    id,
                    family
                        .into_iter()
                        .map(|family| match family {
                            RunFamilyArg::Threads => forgesync_core::EvidenceFamily::Threads,
                            RunFamilyArg::Comments => forgesync_core::EvidenceFamily::Comments,
                            RunFamilyArg::PullRequestMetadata => {
                                forgesync_core::EvidenceFamily::PullRequestMetadata
                            }
                            RunFamilyArg::Reviews => forgesync_core::EvidenceFamily::Reviews,
                            RunFamilyArg::ReviewThreads => {
                                forgesync_core::EvidenceFamily::ReviewThreads
                            }
                        })
                        .collect(),
                    args.json,
                    args.verbose,
                    &cancellation,
                )
                .await;
                interrupt_task.abort();
                result
            }
        },
    }
}

fn thread_filters(
    repositories: Vec<forgesync_engine::RepositorySelector>,
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

async fn sync_command(
    archive_path: &std::path::Path,
    request: SyncRequest,
    json: bool,
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
    let progress_task = if verbose > 0 && !json {
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

struct RefreshCommandRequest<'a> {
    archive_path: &'a std::path::Path,
    repositories: Vec<RepositorySelector>,
    no_sync: bool,
    state: Option<SyncThreadStateArg>,
    with: Vec<SyncIncludeArg>,
    analyze: Vec<RefreshAnalysisArg>,
    force: bool,
    embedding_service: crate::config::EmbeddingServiceConfig,
    recipe: DocumentRecipe,
    json: bool,
    verbose: u8,
    cancellation: &'a tokio_util::sync::CancellationToken,
}

async fn refresh_command(request: RefreshCommandRequest<'_>) -> ExitCode {
    let RefreshCommandRequest {
        archive_path,
        repositories,
        no_sync,
        state,
        with,
        analyze,
        force,
        embedding_service,
        recipe,
        json,
        verbose,
        cancellation,
    } = request;
    if no_sync && analyze.is_empty() {
        return usage_error("refresh requires sync or at least one --analyze stage");
    }
    if analyze
        .iter()
        .enumerate()
        .any(|(index, stage)| analyze[..index].contains(stage))
    {
        return usage_error("refresh analysis stages must be selected only once");
    }
    if no_sync && (state.is_some() || !with.is_empty()) {
        return usage_error("--state and --with require the refresh sync stage");
    }

    let archive = match Archive::open_read_write(archive_path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(json, "refresh", error),
    };
    let clients = if no_sync {
        HashMap::new()
    } else {
        match github_clients_for_selectors(&repositories, verbose, cancellation).await {
            Ok(clients) => clients,
            Err(error) => {
                archive.close().await;
                return render_github_client_setup_error(json, "refresh", error);
            }
        }
    };

    let analysis = analyze
        .into_iter()
        .map(|stage| match stage {
            RefreshAnalysisArg::Embeddings => RefreshAnalysisStage::Embeddings,
            RefreshAnalysisArg::Clusters => RefreshAnalysisStage::Clusters,
        })
        .collect::<Vec<_>>();
    let wants_embeddings = analysis.contains(&RefreshAnalysisStage::Embeddings);
    let wants_clusters = analysis.contains(&RefreshAnalysisStage::Clusters);
    let embedding_client = if wants_embeddings {
        optional_embedding_client(&embedding_service)
    } else {
        None
    };
    let embedding_identity = wants_clusters
        .then(|| configured_embedding_identity(&embedding_service))
        .flatten();
    let sync = (!no_sync).then_some(RefreshSyncOptions {
        scope: match state {
            None => SyncThreadScope::Default,
            Some(SyncThreadStateArg::Open) => SyncThreadScope::Open,
            Some(SyncThreadStateArg::Closed) => SyncThreadScope::Closed,
            Some(SyncThreadStateArg::All) => SyncThreadScope::All,
        },
        include_comments: with.contains(&SyncIncludeArg::Comments),
        include_reviews: with.contains(&SyncIncludeArg::Reviews),
        include_review_threads: with.contains(&SyncIncludeArg::ReviewThreads),
    });
    if verbose > 0 && !json {
        eprintln!("forgesync: refreshing {}", repositories.len());
    }
    let request = RefreshRequest {
        repositories,
        sync,
        analysis,
        recipe,
        embedding_identity,
        force_embeddings: force,
        cluster_options: ClusterOptions::default(),
    };
    let result = refresh(
        &archive,
        &clients,
        embedding_client.as_ref(),
        &request,
        cancellation,
        None,
    )
    .await;
    archive.close().await;
    match result {
        Ok(report) => {
            let exit_status = outcome_exit_code(&report.outcome);
            render_result(json, "refresh", &report, refresh_summary, exit_status)
        }
        Err(error) => render_engine_error(json, "refresh", error),
    }
}

fn optional_embedding_client(
    service: &crate::config::EmbeddingServiceConfig,
) -> Option<EmbeddingClient> {
    service.validate().ok()?;
    let api_key = std::env::var(&service.api_key_env).unwrap_or_default();
    let config = service.client_config(api_key).ok()?;
    EmbeddingClient::new(config).ok()
}

fn configured_embedding_identity(
    service: &crate::config::EmbeddingServiceConfig,
) -> Option<EmbeddingServiceIdentity> {
    service.validate().ok()?;
    let endpoint = url::Url::parse(&service.endpoint).ok()?;
    Some(EmbeddingServiceIdentity {
        endpoint: endpoint.as_str().trim_end_matches('/').to_owned(),
        model: service.model.trim().to_owned(),
    })
}

async fn github_clients_for_selectors(
    selectors: &[RepositorySelector],
    verbose: u8,
    cancellation: &tokio_util::sync::CancellationToken,
) -> Result<HashMap<GitHubHost, GitHubClient>, GitHubClientSetupError> {
    let mut hosts = selectors
        .iter()
        .map(|selector| selector.host().clone())
        .collect::<Vec<_>>();
    hosts.sort();
    hosts.dedup();

    let mut clients = HashMap::with_capacity(hosts.len());
    for host in hosts {
        let token = match crate::credentials::resolve_github_token(
            &crate::credentials::GitHubCredentialSettings::default(),
            &host,
            cancellation,
        )
        .await
        {
            Ok(token) => Some(token),
            Err(
                crate::credentials::CredentialError::NoCredential
                | crate::credentials::CredentialError::CommandUnavailable
                | crate::credentials::CredentialError::CommandFailed
                | crate::credentials::CredentialError::TimedOut,
            ) => {
                if verbose > 0 {
                    eprintln!("forgesync: no usable GitHub token for {host}; trying anonymously");
                }
                None
            }
            Err(crate::credentials::CredentialError::Cancelled) => {
                return Err(GitHubClientSetupError::Cancelled);
            }
            Err(error) => return Err(GitHubClientSetupError::Credential(error.to_string())),
        };
        let base_url = github_api_base_url(&host);
        let config = match url::Url::parse(&base_url) {
            Ok(url) => GitHubClientConfig::new(url),
            Err(_) => return Err(GitHubClientSetupError::InvalidApiUrl),
        };
        match GitHubClient::new(config, token) {
            Ok(client) => {
                clients.insert(host, client);
            }
            Err(error) => return Err(GitHubClientSetupError::Initialization(error.to_string())),
        }
    }
    Ok(clients)
}

#[derive(Debug)]
enum GitHubClientSetupError {
    Cancelled,
    Credential(String),
    InvalidApiUrl,
    Initialization(String),
}

fn render_github_client_setup_error(
    json: bool,
    command: &str,
    error: GitHubClientSetupError,
) -> ExitCode {
    let (code, message, status) = match error {
        GitHubClientSetupError::Cancelled => (
            "operation_cancelled",
            "operation was cancelled before GitHub acquisition began".to_owned(),
            ExitCode::from(130),
        ),
        GitHubClientSetupError::Credential(message) => {
            ("github_credential_invalid", message, ExitCode::FAILURE)
        }
        GitHubClientSetupError::InvalidApiUrl => (
            "github_api_url_invalid",
            "could not build GitHub API URL".to_owned(),
            ExitCode::FAILURE,
        ),
        GitHubClientSetupError::Initialization(message) => (
            "github_client_initialization_failed",
            message,
            ExitCode::FAILURE,
        ),
    };
    render_error_with_status(json, command, code, &message, status)
}

struct EmbedCommandRequest<'a> {
    archive_path: &'a std::path::Path,
    repositories: Vec<RepositorySelector>,
    service: crate::config::EmbeddingServiceConfig,
    recipe: DocumentRecipe,
    force: bool,
    json: bool,
    verbose: u8,
    cancellation: &'a tokio_util::sync::CancellationToken,
}

async fn embed_command(request: EmbedCommandRequest<'_>) -> ExitCode {
    let EmbedCommandRequest {
        archive_path,
        repositories,
        service,
        recipe,
        force,
        json,
        verbose,
        cancellation,
    } = request;
    if let Err(error) = service.validate() {
        return render_error_with_status(
            json,
            "embed",
            error.code(),
            &error.to_string(),
            ExitCode::from(2),
        );
    }
    let api_key = std::env::var(&service.api_key_env).unwrap_or_default();
    let client_config = match service.client_config(api_key) {
        Ok(config) => config,
        Err(error) => {
            return render_error_with_status(
                json,
                "embed",
                error.code(),
                &error.to_string(),
                ExitCode::from(2),
            );
        }
    };
    let client = match EmbeddingClient::new(client_config) {
        Ok(client) => client,
        Err(error) => {
            return render_error_with_status(
                json,
                "embed",
                error.code(),
                &error.to_string(),
                ExitCode::from(2),
            );
        }
    };
    let archive = match Archive::open_read_write(archive_path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(json, "embed", error),
    };
    let mut repositories = repositories
        .into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    repositories.sort_by_key(RepositorySelector::as_url);
    if verbose > 0 && !json {
        eprintln!(
            "forgesync: embedding discussions in {} repository(s)",
            repositories.len()
        );
    }
    let stage = embed_repositories(
        &archive,
        &repositories,
        &client,
        recipe,
        force,
        cancellation,
    )
    .await;
    archive.close().await;
    let Some(stage_report) = stage.report else {
        let failure = stage.failure.unwrap_or(RefreshStageFailure {
            code: "embedding_stage_failed",
            message: "embedding stage did not produce a report".to_owned(),
        });
        let status = if failure.code == "operation_cancelled" {
            ExitCode::from(130)
        } else {
            ExitCode::FAILURE
        };
        return render_error_with_status(json, "embed", failure.code, &failure.message, status);
    };

    let output = EmbeddingOutput {
        repositories: repositories
            .iter()
            .map(RepositorySelector::as_url)
            .collect(),
        recipe,
        endpoint: client.endpoint_identity().to_owned(),
        model: client.model().to_owned(),
        dimensions: service.dimensions,
        status: stage.status,
        report: stage_report.embeddings,
        documents_materialized: stage_report.documents_materialized,
        document_failures: stage_report.document_failures,
        failure: stage.failure,
    };
    let exit_status = match output.status {
        RefreshStageStatus::Complete => ExitCode::SUCCESS,
        RefreshStageStatus::Partial | RefreshStageStatus::Deferred => ExitCode::from(3),
        RefreshStageStatus::Interrupted => ExitCode::from(130),
        RefreshStageStatus::Failed => ExitCode::FAILURE,
    };
    render_result(json, "embed", &output, embedding_summary, exit_status)
}

async fn cluster_command(
    archive_path: &std::path::Path,
    command: ClusterCommand,
    mut embedding_service: crate::config::EmbeddingServiceConfig,
    recipe: DocumentRecipe,
    json: bool,
    verbose: u8,
    cancellation: &tokio_util::sync::CancellationToken,
) -> ExitCode {
    match command {
        ClusterCommand::Build {
            repository,
            endpoint,
            model,
            threshold,
            cross_kind_threshold,
            fanout,
            max_cluster_size,
            min_cluster_size,
        } => {
            if let Some(endpoint) = endpoint {
                embedding_service.endpoint = endpoint;
            }
            if let Some(model) = model {
                embedding_service.model = model;
            }
            if let Err(error) = embedding_service.validate() {
                return render_error_with_status(
                    json,
                    "cluster build",
                    error.code(),
                    &error.to_string(),
                    ExitCode::from(2),
                );
            }
            let endpoint = match url::Url::parse(&embedding_service.endpoint) {
                Ok(endpoint) => endpoint.as_str().trim_end_matches('/').to_owned(),
                Err(_) => {
                    return render_error_with_status(
                        json,
                        "cluster build",
                        "embedding_configuration_invalid",
                        "embedding service configuration is invalid",
                        ExitCode::from(2),
                    );
                }
            };
            let request = ClusterBuildRequest {
                repository,
                endpoint,
                model: embedding_service.model.trim().to_owned(),
                recipe,
                options: ClusterOptions {
                    threshold,
                    cross_kind_threshold,
                    fanout: usize::try_from(fanout).unwrap_or(usize::MAX),
                    max_cluster_size: usize::try_from(max_cluster_size).unwrap_or(usize::MAX),
                    min_cluster_size: usize::try_from(min_cluster_size).unwrap_or(usize::MAX),
                },
            };
            if verbose > 0 && !json {
                eprintln!(
                    "forgesync: building local clusters for {} using stored vectors",
                    request.repository.as_url()
                );
            }
            let archive = match Archive::open_read_write(archive_path).await {
                Ok(archive) => archive,
                Err(error) => return render_store_error(json, "cluster build", error),
            };
            let result = build_clusters(&archive, &request, cancellation).await;
            archive.close().await;
            match result {
                Ok(report) => {
                    let status = if report.generation.complete_coverage {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::from(3)
                    };
                    render_result(
                        json,
                        "cluster build",
                        &report,
                        cluster_build_summary,
                        status,
                    )
                }
                Err(error) => render_engine_error(json, "cluster build", error),
            }
        }
        ClusterCommand::List {
            repositories,
            include_retired,
            limit,
            offset,
        } => match Archive::open_read_only(archive_path).await {
            Ok(archive) => {
                let result = list_clusters(
                    &archive,
                    &ClusterListRequest {
                        repositories,
                        include_retired,
                        limit,
                        offset,
                    },
                )
                .await;
                archive.close().await;
                match result {
                    Ok(page) => render_success(json, "cluster list", &page, cluster_page_summary),
                    Err(error) => render_engine_error(json, "cluster list", error),
                }
            }
            Err(error) => render_store_error(json, "cluster list", error),
        },
        ClusterCommand::Show { id } => match Archive::open_read_only(archive_path).await {
            Ok(archive) => {
                let result = show_cluster(&archive, id).await;
                archive.close().await;
                match result {
                    Ok(detail) => {
                        render_success(json, "cluster show", &detail, cluster_detail_summary)
                    }
                    Err(error) => render_engine_error(json, "cluster show", error),
                }
            }
            Err(error) => render_store_error(json, "cluster show", error),
        },
        ClusterCommand::Dismiss { id, reason } => {
            mutate_cluster(json, "cluster dismiss", id, "dismissed", async {
                let archive = Archive::open_read_write(archive_path).await?;
                let result = dismiss_cluster(&archive, id, reason.as_deref().unwrap_or("")).await;
                archive.close().await;
                result
            })
            .await
        }
        ClusterCommand::Restore { id } => {
            mutate_cluster(json, "cluster restore", id, "restored", async {
                let archive = Archive::open_read_write(archive_path).await?;
                let result = restore_cluster(&archive, id).await;
                archive.close().await;
                result
            })
            .await
        }
        ClusterCommand::Exclude { id, member, reason } => {
            mutate_cluster(json, "cluster exclude", id, "member_excluded", async move {
                let archive = Archive::open_read_write(archive_path).await?;
                let result =
                    exclude_cluster_member(&archive, id, &member, reason.as_deref().unwrap_or(""))
                        .await;
                archive.close().await;
                result
            })
            .await
        }
        ClusterCommand::Include { id, member } => {
            mutate_cluster(json, "cluster include", id, "member_included", async move {
                let archive = Archive::open_read_write(archive_path).await?;
                let result = include_cluster_member(&archive, id, &member).await;
                archive.close().await;
                result
            })
            .await
        }
        ClusterCommand::Canonical { id, member } => {
            mutate_cluster(json, "cluster canonical", id, "canonical_set", async move {
                let archive = Archive::open_read_write(archive_path).await?;
                let result = set_canonical_cluster_member(&archive, id, &member).await;
                archive.close().await;
                result
            })
            .await
        }
    }
}

async fn mutate_cluster<F>(
    json: bool,
    command: &'static str,
    cluster_id: u64,
    action: &'static str,
    operation: F,
) -> ExitCode
where
    F: std::future::Future<Output = Result<(), EngineError>>,
{
    match operation.await {
        Ok(()) => render_success(
            json,
            command,
            &ClusterDecisionOutput { cluster_id, action },
            cluster_decision_summary,
        ),
        Err(EngineError::Store(error)) => render_store_error(json, command, error),
        Err(error) => render_engine_error(json, command, error),
    }
}

async fn retry_command(
    archive_path: &std::path::Path,
    run_id: RunId,
    families: Vec<forgesync_core::EvidenceFamily>,
    json: bool,
    verbose: u8,
    cancellation: &tokio_util::sync::CancellationToken,
) -> ExitCode {
    let archive = match Archive::open_read_write(archive_path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(json, "run retry", error),
    };
    let plan = match plan_run_retry(&archive, run_id, &families).await {
        Ok(plan) => plan,
        Err(error) => {
            archive.close().await;
            return render_engine_error(json, "run retry", error);
        }
    };
    let mut hosts = plan
        .scopes
        .iter()
        .map(|scope| scope.repository.host().clone())
        .collect::<Vec<_>>();
    hosts.sort();
    hosts.dedup();

    let mut clients = HashMap::with_capacity(hosts.len());
    for host in hosts {
        let token = match crate::credentials::resolve_github_token(
            &crate::credentials::GitHubCredentialSettings::default(),
            &host,
            cancellation,
        )
        .await
        {
            Ok(token) => Some(token),
            Err(
                crate::credentials::CredentialError::NoCredential
                | crate::credentials::CredentialError::CommandUnavailable
                | crate::credentials::CredentialError::CommandFailed
                | crate::credentials::CredentialError::TimedOut,
            ) => {
                if verbose > 0 {
                    eprintln!("forgesync: no usable GitHub token for {host}; trying anonymously");
                }
                None
            }
            Err(crate::credentials::CredentialError::Cancelled) => {
                archive.close().await;
                return render_result(
                    json,
                    "run retry",
                    &SyncFailure {
                        code: "operation_cancelled",
                        message: "retry was cancelled before acquisition began".to_owned(),
                    },
                    |failure| failure.message.clone(),
                    ExitCode::from(130),
                );
            }
            Err(error) => {
                archive.close().await;
                return render_error(
                    json,
                    "run retry",
                    "github_credential_invalid",
                    &error.to_string(),
                );
            }
        };
        let config = match url::Url::parse(&github_api_base_url(&host)) {
            Ok(url) => GitHubClientConfig::new(url),
            Err(_) => {
                archive.close().await;
                return render_error(
                    json,
                    "run retry",
                    "github_api_url_invalid",
                    "could not build GitHub API URL",
                );
            }
        };
        match GitHubClient::new(config, token) {
            Ok(client) => {
                clients.insert(host, client);
            }
            Err(error) => {
                archive.close().await;
                return render_error(
                    json,
                    "run retry",
                    "github_client_initialization_failed",
                    &error.to_string(),
                );
            }
        }
    }

    let (progress_sender, mut progress_receiver) = tokio::sync::mpsc::channel::<SyncProgress>(4);
    let progress_task = if verbose > 0 && !json {
        Some(tokio::spawn(async move {
            while let Some(progress) = progress_receiver.recv().await {
                let repository = progress.repository.as_deref().unwrap_or("retry");
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
    let result = run_retry(
        &archive,
        &clients,
        plan,
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
            let exit_status = report
                .runs
                .iter()
                .map(|run| outcome_exit_code(&run.outcome))
                .find(|status| *status != ExitCode::SUCCESS)
                .unwrap_or(ExitCode::SUCCESS);
            render_result(json, "run retry", &report, retry_summary, exit_status)
        }
        Err(error) => render_engine_error(json, "run retry", error),
    }
}

fn github_api_base_url(host: &GitHubHost) -> String {
    if host.as_str() == "github.com" {
        "https://api.github.com/".to_owned()
    } else {
        format!("{}/api/v3/", host.https_origin())
    }
}

fn outcome_exit_code(outcome: &OperationOutcome) -> ExitCode {
    match outcome {
        OperationOutcome::Complete => ExitCode::SUCCESS,
        OperationOutcome::Partial { .. } | OperationOutcome::Deferred { .. } => ExitCode::from(3),
        OperationOutcome::Interrupted { .. } => ExitCode::from(130),
        OperationOutcome::Failed { .. } => ExitCode::FAILURE,
    }
}

fn sync_summary(report: &SyncReport) -> String {
    let state = match report.outcome {
        OperationOutcome::Complete => "complete",
        OperationOutcome::Partial { .. } => "partial",
        OperationOutcome::Deferred { .. } => "deferred",
        OperationOutcome::Interrupted { .. } => "interrupted",
        OperationOutcome::Failed { .. } => "failed",
    };
    format!(
        "Sync {state}: {} repositories, {}/{} jobs complete, {} failed, {} deferred, {} pages, {} threads, {} comments, {} PRs, {} reviews, {} review threads",
        report.repositories_selected,
        report.completed_jobs,
        report.total_jobs,
        report.failed_jobs,
        report.deferred_jobs,
        report.pages_completed,
        report.threads_seen,
        report.comments_seen,
        report.pull_request_metadata_seen,
        report.reviews_seen,
        report.review_threads_seen
    )
}

fn refresh_summary(report: &RefreshReport) -> String {
    let mut parts = Vec::new();
    for selected in &report.selected {
        let (status, detail, failure) = match selected {
            RefreshStageKind::Sync => {
                let Some(stage) = &report.sync else {
                    continue;
                };
                let detail = stage
                    .report
                    .as_ref()
                    .map(|sync| {
                        format!(
                            "{} repositories, {}/{} jobs complete",
                            sync.repositories_selected, sync.completed_jobs, sync.total_jobs
                        )
                    })
                    .unwrap_or_default();
                (stage.status, detail, stage.failure.as_ref())
            }
            RefreshStageKind::Embeddings => {
                let Some(stage) = &report.embeddings else {
                    continue;
                };
                let detail = stage
                    .report
                    .as_ref()
                    .map(|embedding| {
                        format!(
                            "{} documents, {} chunks embedded, {} current, {} failed batches, {} document failures",
                            embedding.embeddings.documents,
                            embedding.embeddings.chunks_embedded,
                            embedding.embeddings.chunks_skipped,
                            embedding.embeddings.failed_batches.len(),
                            embedding.document_failures.len()
                        )
                    })
                    .unwrap_or_default();
                (stage.status, detail, stage.failure.as_ref())
            }
            RefreshStageKind::Clusters => {
                let Some(stage) = &report.clusters else {
                    continue;
                };
                let detail = stage
                    .report
                    .as_ref()
                    .map(|repositories| {
                        let generated = repositories
                            .iter()
                            .filter_map(|repository| repository.report.as_ref())
                            .map(|cluster| cluster.generation.cluster_count)
                            .sum::<u64>();
                        format!(
                            "{} repositories, {generated} generated groups",
                            repositories.len()
                        )
                    })
                    .unwrap_or_default();
                (stage.status, detail, stage.failure.as_ref())
            }
        };
        let name = refresh_stage_name(*selected);
        let mut part = format!("{name} {}", refresh_status_name(status));
        if !detail.is_empty() {
            part.push_str(": ");
            part.push_str(&detail);
        }
        if let Some(failure) = failure {
            part.push_str("; ");
            part.push_str(&failure.message);
        }
        parts.push(part);
    }
    if !report.remaining.is_empty() {
        let remaining = report
            .remaining
            .iter()
            .map(|stage| refresh_stage_name(*stage))
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("remaining: {remaining}"));
    }
    format!(
        "Refresh {}: {}",
        refresh_status_name(refresh_report_status(report)),
        parts.join("; ")
    )
}

fn refresh_report_status(report: &RefreshReport) -> RefreshStageStatus {
    match report.outcome {
        OperationOutcome::Complete => RefreshStageStatus::Complete,
        OperationOutcome::Interrupted { .. } => RefreshStageStatus::Interrupted,
        OperationOutcome::Failed { .. } => RefreshStageStatus::Failed,
        OperationOutcome::Deferred { .. } => RefreshStageStatus::Deferred,
        OperationOutcome::Partial { .. } => RefreshStageStatus::Partial,
    }
}

fn refresh_stage_name(stage: RefreshStageKind) -> &'static str {
    match stage {
        RefreshStageKind::Sync => "sync",
        RefreshStageKind::Embeddings => "embeddings",
        RefreshStageKind::Clusters => "clusters",
    }
}

fn refresh_status_name(status: RefreshStageStatus) -> &'static str {
    match status {
        RefreshStageStatus::Complete => "complete",
        RefreshStageStatus::Partial => "partial",
        RefreshStageStatus::Failed => "failed",
        RefreshStageStatus::Interrupted => "interrupted",
        RefreshStageStatus::Deferred => "deferred",
    }
}

#[derive(Serialize)]
struct SyncFailure {
    code: &'static str,
    message: String,
}

#[derive(Serialize)]
struct EmbeddingOutput {
    repositories: Vec<String>,
    recipe: DocumentRecipe,
    endpoint: String,
    model: String,
    dimensions: Option<u32>,
    status: RefreshStageStatus,
    report: EmbeddingReport,
    documents_materialized: usize,
    document_failures: Vec<RefreshDocumentFailure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure: Option<RefreshStageFailure>,
}

#[derive(Serialize)]
struct ClusterDecisionOutput {
    cluster_id: u64,
    action: &'static str,
}

fn embedding_summary(output: &EmbeddingOutput) -> String {
    let failure = output
        .failure
        .as_ref()
        .map(|failure| failure.message.as_str())
        .or_else(|| {
            output
                .report
                .failed_batches
                .first()
                .map(|failure| failure.message.as_str())
        })
        .or_else(|| {
            output
                .document_failures
                .first()
                .map(|failure| failure.message.as_str())
        });
    let summary = format!(
        "Embedding {}: {} documents, {} chunks embedded, {} already current, {} failed batches, {} document failures using {} ({})",
        refresh_status_name(output.status),
        output.report.documents,
        output.report.chunks_embedded,
        output.report.chunks_skipped,
        output.report.failed_batches.len(),
        output.document_failures.len(),
        output.model,
        output.endpoint
    );
    failure.map_or(summary.clone(), |failure| format!("{summary}; {failure}"))
}

fn cluster_build_summary(report: &ClusterBuildReport) -> String {
    let coverage = if report.generation.complete_coverage {
        "complete"
    } else {
        "partial"
    };
    format!(
        "Cluster build {coverage}: {} eligible discussions, {} with vectors, {} candidate edges, {} groups, {} members, {} groups retired (run {})",
        report.eligible_threads,
        report.vector_threads,
        report.candidate_edges,
        report.generation.cluster_count,
        report.generation.member_count,
        report.generation.retired_count,
        report.generation.run_id
    )
}

fn cluster_page_summary(page: &ClusterPage) -> String {
    if page.items.is_empty() {
        return "No clusters found".to_owned();
    }
    let mut lines = Vec::with_capacity(page.items.len() + 1);
    lines.push(format!("{} cluster(s)", page.items.len()));
    for cluster in &page.items {
        let lifecycle = match cluster.lifecycle {
            forgesync_engine::ClusterLifecycle::Active => "active",
            forgesync_engine::ClusterLifecycle::Retired => "retired",
        };
        let dismissed = if cluster.dismissed { ", dismissed" } else { "" };
        lines.push(format!(
            "#{} [{}{}] {} active / {} excluded: {}",
            cluster.id,
            lifecycle,
            dismissed,
            cluster.active_member_count,
            cluster.excluded_member_count,
            cluster.title
        ));
    }
    if let Some(offset) = page.next_offset {
        lines.push(format!("Next page: --offset {offset}"));
    }
    lines.join("\n")
}

fn cluster_detail_summary(detail: &ClusterDetail) -> String {
    let cluster = &detail.cluster;
    let lifecycle = match cluster.lifecycle {
        forgesync_engine::ClusterLifecycle::Active => "active",
        forgesync_engine::ClusterLifecycle::Retired => "retired",
    };
    let representative = cluster.representative.as_ref().map_or_else(
        || "none".to_owned(),
        |thread| format!("#{}", thread.number().get()),
    );
    let mut lines = vec![format!(
        "Cluster #{} [{}] {} — {} ({} members, representative {})",
        cluster.id,
        lifecycle,
        cluster.repository.full_name,
        cluster.title,
        cluster.active_member_count + cluster.excluded_member_count,
        representative
    )];
    if cluster.dismissed {
        lines.push(format!(
            "Dismissed: {}",
            cluster.dismissal_reason.as_deref().unwrap_or_default()
        ));
    }
    for member in &detail.members {
        let role = match member.role {
            forgesync_engine::ClusterMemberRole::Canonical => "canonical",
            forgesync_engine::ClusterMemberRole::Representative => "representative",
            forgesync_engine::ClusterMemberRole::Related => "related",
        };
        let state = match member.state {
            forgesync_engine::ClusterMemberState::Active => "active",
            forgesync_engine::ClusterMemberState::Excluded => "excluded",
            forgesync_engine::ClusterMemberState::Removed => "removed",
        };
        lines.push(format!(
            "  #{} [{role}, {state}] {}",
            member.summary.discussion.id.number().get(),
            member.summary.discussion.title
        ));
    }
    lines.join("\n")
}

fn cluster_decision_summary(output: &ClusterDecisionOutput) -> String {
    format!("Cluster #{}: {}", output.cluster_id, output.action)
}

fn render_thread_page(json: bool, command: &str, page: &ThreadPage) -> ExitCode {
    let output = ThreadPageOutput::from(page);
    render_success(json, command, &output, thread_page_summary)
}

fn render_search_page(json: bool, page: &SearchResultPage) -> ExitCode {
    let output = SearchPageOutput::from(page);
    render_success(json, "search", &output, search_page_summary)
}

fn render_thread_detail(json: bool, detail: &ThreadDetail) -> ExitCode {
    let output = ThreadDetailOutput::from(detail);
    render_success(json, "thread show", &output, thread_detail_summary)
}

fn archive_status_summary(status: &ArchiveStatusOutput<'_>) -> String {
    let mut lines = vec![
        archive_summary(status.archive),
        format!(
            "Repositories: {}\nThreads: {} ({} issues, {} pull requests)",
            status.repositories, status.threads, status.issues, status.pull_requests
        ),
        "Coverage:".to_owned(),
    ];
    lines.extend(status.coverage.iter().map(|coverage| {
        format!(
            "  {}: {} complete, {} incomplete, {} missing of {}",
            family_name(coverage.family),
            coverage.complete,
            coverage.incomplete,
            coverage.missing,
            coverage.applicable_threads
        )
    }));
    lines.push(format!(
        "Work: {} unresolved failures, {} failed jobs, {} deferred jobs, {} in-progress runs",
        status.diagnostics.work.unresolved_failures,
        status.diagnostics.work.failed_jobs,
        status.diagnostics.work.deferred_jobs,
        status.diagnostics.work.in_progress_runs
    ));
    lines.push(format!(
        "Lease: {} (fence {}, expires {})",
        if status.diagnostics.lease.held {
            format!(
                "held by {}",
                status
                    .diagnostics
                    .lease
                    .owner_id
                    .as_deref()
                    .unwrap_or("unknown")
            )
        } else {
            "available".to_owned()
        },
        status.diagnostics.lease.fencing_token,
        status
            .diagnostics
            .lease
            .expires_at
            .format_rfc3339()
            .unwrap_or_else(|_| "invalid timestamp".to_owned())
    ));
    lines.push(format!(
        "Schema: {} / {} supported{}",
        status.diagnostics.schema.current_version,
        status.diagnostics.schema.supported_version,
        if status.diagnostics.schema.history_valid {
            " (history valid)"
        } else {
            " (history invalid)"
        }
    ));
    lines.join("\n")
}

fn run_list_summary(runs: &Vec<RunRecord>) -> String {
    let mut lines = vec!["ID\tSTATUS\tSTARTED\tPARENT".to_owned()];
    for run in runs {
        lines.push(format!(
            "{}\t{}\t{}\t{}",
            run.id.get(),
            run_status_name(run.status),
            run.started_at
                .format_rfc3339()
                .unwrap_or_else(|_| "invalid timestamp".to_owned()),
            run.parent_id
                .map(|parent| parent.get().to_string())
                .unwrap_or_else(|| "-".to_owned())
        ));
    }
    if runs.is_empty() {
        lines.push("No runs recorded.".to_owned());
    }
    lines.join("\n")
}

fn run_detail_summary(detail: &RunDetail) -> String {
    let mut lines = vec![format!(
        "Run {}: {}\nStarted: {}\nParent: {}\nJobs: {}\nFailures: {}",
        detail.run.id.get(),
        run_status_name(detail.run.status),
        detail
            .run
            .started_at
            .format_rfc3339()
            .unwrap_or_else(|_| "invalid timestamp".to_owned()),
        detail
            .run
            .parent_id
            .map(|parent| parent.get().to_string())
            .unwrap_or_else(|| "-".to_owned()),
        detail.jobs.len(),
        detail.failures.len()
    )];
    lines.push("Jobs:".to_owned());
    lines.extend(detail.jobs.iter().map(|job| {
        format!(
            "  {} {} [{}]: {} ({} items, {} pages)",
            job.repository.full_name,
            family_name(job.family),
            job.scope_key,
            sync_job_status_name(job.status),
            job.items_committed,
            job.pages_completed
        )
    }));
    lines.push("Failures:".to_owned());
    lines.extend(detail.failures.iter().map(|failure| {
        format!(
            "  {} {} [{}]: {}{}",
            failure.target,
            failure.family.map(family_name).unwrap_or("unassigned"),
            failure.scope_key,
            failure.failure.message,
            if failure.resolved_at.is_some() {
                " (resolved)"
            } else {
                ""
            }
        )
    }));
    if detail.failures.is_empty() {
        lines.push("  None".to_owned());
    }
    lines.join("\n")
}

fn retry_summary(report: &RetryReport) -> String {
    let mut lines = vec![format!(
        "Retry of run {}: {} failure(s), {} sync run(s)",
        report.parent_run_id.get(),
        report.failure_ids.len(),
        report.runs.len()
    )];
    lines.extend(report.runs.iter().map(sync_summary));
    lines.join("\n")
}

fn run_status_name(status: RunStatus) -> &'static str {
    match status {
        RunStatus::InProgress => "in_progress",
        RunStatus::Complete => "complete",
        RunStatus::Partial => "partial",
        RunStatus::Failed => "failed",
        RunStatus::Interrupted => "interrupted",
        RunStatus::Deferred => "deferred",
    }
}

fn sync_job_status_name(status: SyncJobStatus) -> &'static str {
    match status {
        SyncJobStatus::Pending => "pending",
        SyncJobStatus::InProgress => "in_progress",
        SyncJobStatus::Complete => "complete",
        SyncJobStatus::Failed => "failed",
        SyncJobStatus::Deferred => "deferred",
        SyncJobStatus::Interrupted => "interrupted",
    }
}

fn thread_page_summary(page: &ThreadPageOutput<'_>) -> String {
    let mut lines = vec!["REPOSITORY\tNUMBER\tKIND\tSTATE\tTITLE".to_owned()];
    for item in &page.items {
        let thread = item.thread;
        lines.push(format!(
            "{}\t{}\t{}\t{}\t{}",
            item.repository.full_name,
            thread.id.number().get(),
            discussion_kind_name(thread.kind),
            source_state_name(&thread.state),
            thread.title
        ));
    }
    if page.items.is_empty() {
        lines.push("No discussions matched.".to_owned());
    }
    lines.push("Coverage:".to_owned());
    lines.extend(page.coverage.iter().map(|coverage| {
        format!(
            "  {}: {} complete, {} incomplete, {} missing of {}",
            family_name(coverage.family),
            coverage.complete,
            coverage.incomplete,
            coverage.missing,
            coverage.applicable_threads
        )
    }));
    if let Some(next_offset) = page.next_offset {
        lines.push(format!("Next offset: {next_offset}"));
    }
    lines.join("\n")
}

fn search_page_summary(page: &SearchPageOutput<'_>) -> String {
    let mut lines = vec![format!(
        "Mode: {:?} (requested {:?}), ranking: {:?}, sort: {:?}",
        page.mode, page.requested_mode, page.ranking, page.sort
    )];
    if let Some(reason) = page.fallback_reason {
        lines.push(format!("Keyword fallback: {reason}"));
    }
    lines.push("RANK\tSCORE\tREPOSITORY\tNUMBER\tKIND\tSTATE\tTITLE".to_owned());
    for (index, item) in page.items.iter().enumerate() {
        let score = item
            .score
            .map(|score| format!("{score:.6}"))
            .unwrap_or_else(|| "-".to_owned());
        lines.push(format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            index + 1,
            score,
            item.repository.full_name,
            item.thread.id.number().get(),
            discussion_kind_name(item.thread.kind),
            source_state_name(&item.thread.state),
            item.thread.title
        ));
    }
    if page.items.is_empty() {
        lines.push("No discussions matched.".to_owned());
    }
    lines.push("Coverage:".to_owned());
    lines.extend(page.coverage.iter().map(|coverage| {
        format!(
            "  {}: {} complete, {} incomplete, {} missing of {}",
            family_name(coverage.family),
            coverage.complete,
            coverage.incomplete,
            coverage.missing,
            coverage.applicable_threads
        )
    }));
    if let Some(next_offset) = page.next_offset {
        lines.push(format!("Next offset: {next_offset}"));
    }
    lines.join("\n")
}

fn thread_detail_summary(detail: &ThreadDetailOutput<'_>) -> String {
    let thread = detail.summary.thread;
    let mut lines = vec![format!(
        "{}/{}#{} — {}\nKind: {}\nState: {}\nUpdated: {}",
        detail.summary.repository.owner,
        detail.summary.repository.name,
        thread.id.number().get(),
        thread.title,
        discussion_kind_name(thread.kind),
        source_state_name(&thread.state),
        format_timestamp(thread.updated_at)
    )];
    if let Some(url) = &thread.html_url {
        lines.push(format!("URL: {url}"));
    }
    if let Some(body) = &thread.body {
        lines.push(String::new());
        lines.push(body.clone());
    }
    lines.push(String::new());
    lines.push("Coverage:".to_owned());
    lines.extend(detail.summary.coverage.iter().map(|coverage| {
        format!(
            "  {}: {}{}",
            family_name(coverage.family()),
            coverage_state_name(coverage.state()),
            if coverage.is_stale() { " (stale)" } else { "" }
        )
    }));
    for item in detail.pull_request_metadata {
        let metadata = &item.payload;
        lines.push(String::new());
        lines.push(format!(
            "Pull request: {}:{} -> {}:{} (head {}, draft: {}, merged: {})",
            repository_identity(metadata.head.repository.as_ref()),
            metadata.head.name,
            repository_identity(metadata.base.repository.as_ref()),
            metadata.base.name,
            metadata.head.sha,
            metadata.draft,
            metadata.merged
        ));
    }

    if !detail.timeline.is_empty() {
        lines.push(String::new());
        lines.push("Current timeline:".to_owned());
        for entry in detail.timeline {
            let time = entry
                .occurred_at
                .map(format_timestamp)
                .unwrap_or_else(|| "time unavailable".to_owned());
            let summary = match &entry.event {
                ThreadTimelineEvent::ThreadCreated { thread, title } => {
                    format!(
                        "{}#{} opened: {title}",
                        thread.repository().provider_id(),
                        thread.number().get()
                    )
                }
                ThreadTimelineEvent::ThreadClosed { thread } => {
                    format!(
                        "{}#{} closed",
                        thread.repository().provider_id(),
                        thread.number().get()
                    )
                }
                ThreadTimelineEvent::Comment { comment } => format!(
                    "comment by {}: {}",
                    comment.author.as_deref().unwrap_or("unknown author"),
                    comment.body
                ),
                ThreadTimelineEvent::Review { review } => format!(
                    "review {} by {}: {}{}",
                    review.id.provider_id(),
                    review
                        .reviewer
                        .as_ref()
                        .and_then(|reviewer| reviewer.login.as_deref())
                        .unwrap_or("unknown reviewer"),
                    review_state_name(&review.state),
                    review
                        .body
                        .as_deref()
                        .map_or(String::new(), |body| format!(" — {body}"))
                ),
                ThreadTimelineEvent::ReviewThread {
                    path,
                    is_resolved,
                    is_outdated,
                    ..
                } => format!(
                    "review thread {}: {}{}",
                    path.as_deref().unwrap_or("unknown path"),
                    if *is_resolved {
                        "resolved"
                    } else {
                        "unresolved"
                    },
                    if *is_outdated { ", outdated" } else { "" }
                ),
                ThreadTimelineEvent::ReviewThreadComment {
                    path,
                    is_resolved,
                    is_outdated,
                    comment,
                    ..
                } => format!(
                    "review comment on {} ({}{}), by {}: {}",
                    path.as_deref().unwrap_or("unknown path"),
                    if *is_resolved {
                        "resolved"
                    } else {
                        "unresolved"
                    },
                    if *is_outdated { ", outdated" } else { "" },
                    comment.author.as_deref().unwrap_or("unknown author"),
                    comment.body
                ),
            };
            lines.push(format!("  {time}: {summary}"));
        }
    }
    lines.join("\n")
}

fn discussion_kind_name(kind: DiscussionKind) -> &'static str {
    match kind {
        DiscussionKind::Issue => "issue",
        DiscussionKind::PullRequest => "pull request",
    }
}

fn source_state_name(state: &SourceState) -> &str {
    match state {
        SourceState::Open => "open",
        SourceState::Closed => "closed",
        SourceState::Other(value) => value,
    }
}

fn review_state_name(state: &ReviewState) -> &str {
    match state {
        ReviewState::Approved => "approved",
        ReviewState::ChangesRequested => "changes requested",
        ReviewState::Commented => "commented",
        ReviewState::Dismissed => "dismissed",
        ReviewState::Pending => "pending",
        ReviewState::Other(value) => value,
    }
}

fn repository_identity(repository: Option<&forgesync_core::RepositoryId>) -> String {
    repository.map_or_else(
        || "unknown repository".to_owned(),
        |repository| {
            format!(
                "{}/{}",
                repository.host(),
                repository.provider_id().as_str()
            )
        },
    )
}

fn family_name(family: forgesync_core::EvidenceFamily) -> &'static str {
    match family {
        forgesync_core::EvidenceFamily::Threads => "threads",
        forgesync_core::EvidenceFamily::Comments => "comments",
        forgesync_core::EvidenceFamily::PullRequestMetadata => "pull_request_metadata",
        forgesync_core::EvidenceFamily::Reviews => "reviews",
        forgesync_core::EvidenceFamily::ReviewThreads => "review_threads",
    }
}

fn coverage_state_name(state: &CoverageState) -> &'static str {
    match state {
        CoverageState::Missing => "missing",
        CoverageState::Incomplete { .. } => "incomplete",
        CoverageState::Complete { .. } => "complete",
        CoverageState::Unavailable { .. } => "unavailable",
        CoverageState::Failed { .. } => "failed",
        CoverageState::Deferred { .. } => "deferred",
    }
}

fn format_timestamp(timestamp: UtcTimestamp) -> String {
    timestamp
        .format_rfc3339()
        .unwrap_or_else(|_| "invalid timestamp".to_owned())
}

#[derive(Serialize)]
struct MigrationOutput {
    migration: MigrationReport,
    archive: ArchiveInfo,
}

fn archive_summary(info: &ArchiveInfo) -> String {
    format!(
        "Archive: {}\nID: {}\nFormat: {}\nSchema: {}\nCreated: {}\nSQLite: {}",
        info.path,
        info.archive_id,
        info.format_id,
        info.schema_version,
        info.created_at
            .format_rfc3339()
            .unwrap_or_else(|_| "invalid timestamp".to_owned()),
        info.sqlite_version
    )
}

fn migration_summary(output: &MigrationOutput) -> String {
    let applied = output.migration.applied_migrations.len();
    format!(
        "Archive schema is at version {} ({} migration{} applied)",
        output.migration.schema_version,
        applied,
        if applied == 1 { "" } else { "s" }
    )
}

fn doctor_summary(report: &DoctorReport) -> String {
    let headline = if report.healthy {
        "Archive health: healthy"
    } else {
        "Archive health: unhealthy"
    };
    let checks = report
        .checks
        .iter()
        .map(|check| {
            let state = if check.healthy { "ok" } else { "failed" };
            format!("[{state}] {}: {}", check.name, check.detail)
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{headline}\n{checks}\n{} unresolved failures, {} failed jobs, {} deferred jobs\nSchema {} / {}, lease {}",
        report.diagnostics.work.unresolved_failures,
        report.diagnostics.work.failed_jobs,
        report.diagnostics.work.deferred_jobs,
        report.diagnostics.schema.current_version,
        report.diagnostics.schema.supported_version,
        if report.diagnostics.lease.held {
            "held"
        } else {
            "available"
        }
    )
}

fn render_success<T>(
    json: bool,
    command: &str,
    data: &T,
    human: impl FnOnce(&T) -> String,
) -> ExitCode
where
    T: Serialize,
{
    render_result(json, command, data, human, ExitCode::SUCCESS)
}

fn render_result<T>(
    json: bool,
    command: &str,
    data: &T,
    human: impl FnOnce(&T) -> String,
    exit_status: ExitCode,
) -> ExitCode
where
    T: Serialize,
{
    let mut stdout = std::io::stdout().lock();
    let result = if json {
        let envelope = JsonEnvelope::success(command, data);
        serde_json::to_writer(&mut stdout, &envelope)
            .and_then(|()| writeln!(stdout).map_err(serde_json::Error::io))
    } else {
        writeln!(stdout, "{}", human(data)).map_err(serde_json::Error::io)
    };
    match result {
        Ok(()) => exit_status,
        Err(error) if error.io_error_kind() == Some(std::io::ErrorKind::BrokenPipe) => exit_status,
        Err(_) => ExitCode::FAILURE,
    }
}

fn render_store_error(json: bool, command: &str, error: StoreError) -> ExitCode {
    render_error(json, command, error.code(), &error.to_string())
}

fn render_engine_error(json: bool, command: &str, error: EngineError) -> ExitCode {
    let code = error.code();
    let message = error.to_string();
    let status = if code == "operation_cancelled" {
        ExitCode::from(130)
    } else {
        ExitCode::FAILURE
    };
    render_error_with_status(json, command, code, &message, status)
}

fn render_error(json: bool, command: &str, code: &str, message: &str) -> ExitCode {
    render_error_with_status(json, command, code, message, ExitCode::FAILURE)
}

fn render_error_with_status(
    json: bool,
    command: &str,
    code: &str,
    message: &str,
    exit_status: ExitCode,
) -> ExitCode {
    if json {
        let envelope = JsonEnvelope::<serde_json::Value>::failure(command, code, message);
        if serde_json::to_writer(std::io::stdout().lock(), &envelope).is_ok() {
            let _ = writeln!(std::io::stdout().lock());
        }
    } else {
        let _ = writeln!(std::io::stderr().lock(), "forgesync: {message}");
    }
    exit_status
}

fn usage_error(message: &str) -> ExitCode {
    let mut command = CliArgs::command();
    let error = command.error(ErrorKind::MissingRequiredArgument, message.to_owned());
    let code = error.exit_code();
    let _ = error.print();
    ExitCode::from(u8::try_from(code).unwrap_or(2))
}
