//! Refresh command handling.

use std::collections::HashMap;
use std::process::ExitCode;

use forgesync_engine::clustering::ClusterOptions;
use forgesync_engine::embedding_client::EmbeddingClient;
use forgesync_engine::refresh::{
    EmbeddingServiceIdentity, RefreshAnalysisStage, RefreshRequest, RefreshSyncOptions, refresh,
};
use forgesync_engine::sync::SyncThreadScope;
use forgesync_store::archive::Archive;

use super::github::{github_clients_for_selectors, render_github_client_setup_error};
use crate::args::{RefreshAnalysisArg, RefreshArgs, SyncIncludeArg, SyncThreadStateArg};
use crate::config::ForgesyncConfig;
use crate::reports::{outcome_exit_code, refresh_summary};
use crate::{OutputMode, render_engine_error, render_result, render_store_error, usage_error};

pub(super) async fn refresh_from_cli(
    args: RefreshArgs,
    path: &std::path::Path,
    json: OutputMode,
    verbose: u8,
    config: ForgesyncConfig,
) -> ExitCode {
    let cancellation = tokio_util::sync::CancellationToken::new();
    let interrupt_cancellation = cancellation.clone();
    let interrupt_task = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            interrupt_cancellation.cancel();
        }
    });
    let result = refresh_command(args, path, json, verbose, config, &cancellation).await;
    interrupt_task.abort();
    result
}

async fn refresh_command(
    args: RefreshArgs,
    path: &std::path::Path,
    json: OutputMode,
    verbose: u8,
    config: ForgesyncConfig,
    cancellation: &tokio_util::sync::CancellationToken,
) -> ExitCode {
    let RefreshArgs {
        repositories,
        no_sync,
        state,
        with,
        analyze,
        force,
    } = args;
    let embedding_service = config.embeddings;
    let recipe = config.documents.recipe;
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

    let archive = match Archive::open_read_write(path).await {
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
