//! Refresh command handling.

use super::*;

pub(super) struct RefreshCliRequest<'a> {
    pub(super) archive_path: &'a std::path::Path,
    pub(super) repositories: Vec<RepositorySelector>,
    pub(super) no_sync: bool,
    pub(super) state: Option<SyncThreadStateArg>,
    pub(super) with: Vec<SyncIncludeArg>,
    pub(super) analyze: Vec<RefreshAnalysisArg>,
    pub(super) force: bool,
    pub(super) embedding_service: crate::config::EmbeddingServiceConfig,
    pub(super) recipe: DocumentRecipe,
    pub(super) json: OutputMode,
    pub(super) verbose: u8,
}

pub(super) async fn refresh_from_cli(request: RefreshCliRequest<'_>) -> ExitCode {
    let cancellation = tokio_util::sync::CancellationToken::new();
    let interrupt_cancellation = cancellation.clone();
    let interrupt_task = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            interrupt_cancellation.cancel();
        }
    });
    let result = refresh_command(RefreshCommandRequest {
        archive_path: request.archive_path,
        repositories: request.repositories,
        no_sync: request.no_sync,
        state: request.state,
        with: request.with,
        analyze: request.analyze,
        force: request.force,
        embedding_service: request.embedding_service,
        recipe: request.recipe,
        json: request.json,
        verbose: request.verbose,
        cancellation: &cancellation,
    })
    .await;
    interrupt_task.abort();
    result
}

pub(super) struct RefreshCommandRequest<'a> {
    pub(super) archive_path: &'a std::path::Path,
    pub(super) repositories: Vec<RepositorySelector>,
    pub(super) no_sync: bool,
    pub(super) state: Option<SyncThreadStateArg>,
    pub(super) with: Vec<SyncIncludeArg>,
    pub(super) analyze: Vec<RefreshAnalysisArg>,
    pub(super) force: bool,
    pub(super) embedding_service: crate::config::EmbeddingServiceConfig,
    pub(super) recipe: DocumentRecipe,
    pub(super) json: OutputMode,
    pub(super) verbose: u8,
    pub(super) cancellation: &'a tokio_util::sync::CancellationToken,
}

pub(super) async fn refresh_command(request: RefreshCommandRequest<'_>) -> ExitCode {
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
    if verbose > 0 && !json.is_json() {
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

pub(super) fn optional_embedding_client(
    service: &crate::config::EmbeddingServiceConfig,
) -> Option<EmbeddingClient> {
    service.validate().ok()?;
    let api_key = std::env::var(&service.api_key_env).unwrap_or_default();
    let config = service.client_config(api_key).ok()?;
    EmbeddingClient::new(config).ok()
}

pub(super) fn configured_embedding_identity(
    service: &crate::config::EmbeddingServiceConfig,
) -> Option<EmbeddingServiceIdentity> {
    service.validate().ok()?;
    let endpoint = url::Url::parse(&service.endpoint).ok()?;
    Some(EmbeddingServiceIdentity {
        endpoint: endpoint.as_str().trim_end_matches('/').to_owned(),
        model: service.model.trim().to_owned(),
    })
}
