//! Search command setup and execution.

use super::*;

pub(super) struct SearchCommandRequest<'a> {
    pub(super) path: &'a std::path::Path,
    pub(super) json: bool,
    pub(super) service: crate::config::EmbeddingServiceConfig,
    pub(super) recipe: DocumentRecipe,
    pub(super) query: String,
    pub(super) repositories: Vec<RepositorySelector>,
    pub(super) kind: Option<ThreadKindArg>,
    pub(super) state: ThreadStateArg,
    pub(super) mode: SearchModeArg,
    pub(super) keyword_fallback: bool,
    pub(super) sort: Option<ThreadSortArg>,
    pub(super) limit: u32,
    pub(super) offset: u64,
}

pub(super) async fn search_command(request: SearchCommandRequest<'_>) -> ExitCode {
    let SearchCommandRequest {
        path,
        json,
        service,
        recipe,
        query,
        repositories,
        kind,
        state,
        mode,
        keyword_fallback,
        sort,
        limit,
        offset,
    } = request;
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
