//! # Search the local archive
//!
//! `SearchArgs` selects query text, repository scope, mode, ranking, and output shape. Its run
//! method creates an engine search request and renders a page of hits with provenance.
//!
//! Search remains offline. Keyword and semantic results use stored documents and embeddings; this
//! command does not fetch GitHub data or call the embedding service behind the user's back.

use std::process::ExitCode;

use clap::{ArgAction, Args};
use forgesync_core::document::DocumentRecipe;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::search::{SearchMode, SearchRequest, retrieve_threads};
use forgesync_store::archive::Archive;

use super::thread::thread_filters;
use super::{SearchModeArg, ThreadKindArg, ThreadSortArg, ThreadStateArg};
use crate::reports::render_search_page;
use crate::{OutputMode, render_engine_error, render_error_with_status, render_store_error};

/// Search archived discussions with local keyword or optional semantic ranking.
#[derive(Clone, Debug, Args)]
pub struct SearchArgs {
    /// Search text sent to the selected local or semantic retrieval mode.
    pub query: String,
    /// Limit results to one or more registered repositories.
    #[arg(long = "repo", value_name = "OWNER/REPO")]
    pub repositories: Vec<RepositorySelector>,
    /// Limit results to issues or pull requests.
    #[arg(long, value_enum)]
    pub kind: Option<ThreadKindArg>,
    /// Filter by source open or closed state.
    #[arg(long, value_enum, default_value_t = ThreadStateArg::All)]
    pub state: ThreadStateArg,
    /// Choose keyword, semantic, hybrid, or explicit FTS5 retrieval.
    #[arg(long, value_enum, default_value_t = SearchModeArg::Keyword)]
    pub mode: SearchModeArg,
    /// Return keyword results if semantic retrieval has no compatible data or fails.
    #[arg(long, action = ArgAction::SetTrue)]
    pub keyword_fallback: bool,
    /// Sort results by relevance, source update time, or creation time.
    #[arg(long, value_enum)]
    pub sort: Option<ThreadSortArg>,
    /// Maximum number of results (1-1000).
    #[arg(
        long,
        default_value_t = 20,
        value_parser = clap::value_parser!(u32).range(1..=1000)
    )]
    pub limit: u32,
    /// Number of matching rows to skip.
    #[arg(long, default_value_t = 0)]
    pub offset: u64,
}

impl SearchArgs {
    /// Resolves the selected search mode, reads the archive, and renders one result page.
    pub async fn run(
        self,
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
        } = self;
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
            match service.client() {
                Ok(client) => Some(client),
                Err(error) => {
                    let status = match error {
                        super::embedding_service::EmbeddingSetupError::Configuration(_) => {
                            ExitCode::from(2)
                        }
                        super::embedding_service::EmbeddingSetupError::Client(_) => {
                            ExitCode::FAILURE
                        }
                    };
                    return render_error_with_status(
                        json,
                        "search",
                        error.code(),
                        &error.to_string(),
                        status,
                    );
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
}
