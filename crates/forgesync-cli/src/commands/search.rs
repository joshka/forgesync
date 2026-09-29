//! Search command setup and execution.

use std::process::ExitCode;

use forgesync_core::document::DocumentRecipe;
use forgesync_engine::embedding_client::EmbeddingClient;
use forgesync_engine::search::{SearchMode, SearchRequest, retrieve_threads};
use forgesync_store::archive::Archive;

use crate::args::{SearchArgs, SearchModeArg};
use crate::commands::thread_filters;
use crate::reports::render_search_page;
use crate::{
    OutputMode, render_engine_error, render_error, render_error_with_status, render_store_error,
};

pub async fn search_command(
    args: SearchArgs,
    path: &std::path::Path,
    json: OutputMode,
    service: crate::config::EmbeddingServiceConfig,
    recipe: DocumentRecipe,
) -> ExitCode {
    let SearchArgs {
        query,
        repositories,
        kind,
        state,
        mode,
        keyword_fallback,
        sort,
        limit,
        offset,
    } = args;
    let mode = match mode {
        SearchModeArg::Keyword => SearchMode::Keyword,
        SearchModeArg::AdvancedFts => SearchMode::AdvancedFts,
        SearchModeArg::Semantic => SearchMode::Semantic,
        SearchModeArg::Hybrid => SearchMode::Hybrid,
    };
    if keyword_fallback && !matches!(mode, SearchMode::Semantic | SearchMode::Hybrid) {
        return render_error_with_status(
            json,
            "search",
            "search_fallback_mode_invalid",
            "--keyword-fallback requires --mode semantic or --mode hybrid",
            ExitCode::from(2),
        );
    }
    let embedding_client = if matches!(mode, SearchMode::Semantic | SearchMode::Hybrid) {
        let service = service.clone();
        let api_key = std::env::var(&service.api_key_env).unwrap_or_default();
        let client_config = match service.client_config(api_key) {
            Ok(config) => config,
            Err(error) => {
                return render_error_with_status(
                    json,
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
                return render_error(json, "search", error.code(), &error.to_string());
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
    let result = match Archive::open_read_only(path).await {
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
                recipe,
                embedding_client.as_ref(),
                &cancellation,
            )
            .await;
            archive.close().await;
            match result {
                Ok(page) => render_search_page(json, &page),
                Err(error) => render_engine_error(json, "search", error),
            }
        }
        Err(error) => render_store_error(json, "search", error),
    };
    interrupt_task.abort();
    result
}
