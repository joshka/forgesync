//! Refresh arguments and process behavior.

use std::collections::HashMap;
use std::process::ExitCode;

use clap::{ArgAction, Args};
use forgesync_engine::clustering::ClusterOptions;
use forgesync_engine::embedding_client::EmbeddingClient;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::refresh::{
    EmbeddingServiceIdentity, RefreshAnalysisStage, RefreshRequest, RefreshSyncOptions, refresh,
};
use forgesync_engine::sync::SyncThreadScope;
use forgesync_store::archive::Archive;

use super::github::{github_clients_for_selectors, render_github_client_setup_error};
use super::{RefreshAnalysisArg, SyncIncludeArg, SyncThreadStateArg};
use crate::config::ForgesyncConfig;
use crate::reports::{outcome_exit_code, refresh_summary};
use crate::{OutputMode, render_engine_error, render_result, render_store_error, usage_error};

/// Sync a repository and run explicitly selected local analysis stages.
#[derive(Clone, Debug, Args)]
pub struct RefreshArgs {
    /// Repository scope shared by sync, embedding, and clustering stages.
    #[arg(value_name = "OWNER/REPO", required = true)]
    pub repositories: Vec<RepositorySelector>,
    /// Skip GitHub acquisition and analyze only the local archive.
    #[arg(long, action = ArgAction::SetTrue)]
    pub no_sync: bool,
    /// Select open threads, closed threads, or a complete all-state enumeration.
    #[arg(long, value_enum)]
    pub state: Option<SyncThreadStateArg>,
    /// Add selected evidence families to the sync stage.
    #[arg(long = "with", value_enum, value_delimiter = ',')]
    pub with: Vec<SyncIncludeArg>,
    /// Explicitly select model-backed stages; clustering uses stored vectors.
    #[arg(long, value_enum, value_delimiter = ',')]
    pub analyze: Vec<RefreshAnalysisArg>,
    /// Force embedding requests even when compatible vectors are stored.
    #[arg(long, action = ArgAction::SetTrue)]
    pub force: bool,
}

impl RefreshArgs {
    /// Runs the selected refresh workflow with process cancellation and result rendering.
    pub async fn run(
        self,
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
        let result = execute_refresh(self, path, json, verbose, config, &cancellation).await;
        interrupt_task.abort();
        result
    }
}

/// Executes one prepared refresh request against the selected archive.
async fn execute_refresh(
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

/// Builds an embedding client when the configured service is usable.
///
/// Invalid optional configuration returns `None` so a refresh without embedding analysis can
/// proceed. The selected embedding stage reports an unavailable service if it needs this client.
pub fn optional_embedding_client(
    service: &crate::config::EmbeddingServiceConfig,
) -> Option<EmbeddingClient> {
    service.validate().ok()?;
    let api_key = std::env::var(&service.api_key_env).unwrap_or_default();
    let config = service.client_config(api_key).ok()?;
    EmbeddingClient::new(config).ok()
}

/// Derives the stable endpoint and model identity used to select compatible stored vectors.
pub fn configured_embedding_identity(
    service: &crate::config::EmbeddingServiceConfig,
) -> Option<EmbeddingServiceIdentity> {
    service.validate().ok()?;
    let endpoint = url::Url::parse(&service.endpoint).ok()?;
    Some(EmbeddingServiceIdentity {
        endpoint: endpoint.as_str().trim_end_matches('/').to_owned(),
        model: service.model.trim().to_owned(),
    })
}
