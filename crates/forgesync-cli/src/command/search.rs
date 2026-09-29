//! # Search the local archive
//!
//! `SearchArgs` selects query text, repository scope, mode, ranking, and output shape. Its run
//! method creates an engine search request and renders a page of hits with provenance.
//!
//! Keyword and advanced FTS reads remain offline. Semantic and hybrid modes use an explicitly
//! configured service to embed the query, then compare it with stored discussion vectors. No mode
//! fetches GitHub data or writes to the archive.
//!
//! Preparation validates fallback policy before service setup. `PreparedSearch` couples the
//! engine request with its selected recipe and optional client; execution owns the read-only
//! archive and closes it before rendering either a page or an error.

use std::process::ExitCode;

use clap::{ArgAction, Args};
use forgesync_core::document::DocumentRecipe;
use forgesync_engine::embedding_client::EmbeddingClient;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::search::{SearchMode, SearchRequest, retrieve_threads};
use forgesync_store::archive::Archive;

use super::embedding_service::EmbeddingSetupError;
use super::interruption::CommandInterruption;
use super::thread::thread_filters;
use super::{SearchModeArg, ThreadKindArg, ThreadSortArg, ThreadStateArg};
use crate::config::EmbeddingServiceConfig;
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
    /// Validates search policy, prepares query embedding when needed, and renders a result page.
    ///
    /// Invalid fallback policy is a usage error even if the archive or service is unavailable.
    /// Service setup does not send a request; the engine sends it during semantic retrieval.
    pub async fn run(
        self,
        path: &std::path::Path,
        json: OutputMode,
        service: EmbeddingServiceConfig,
        recipe: DocumentRecipe,
    ) -> ExitCode {
        let request = self.request();
        if request.allow_keyword_fallback && !requires_embedding(request.mode) {
            return invalid_fallback(json);
        }
        let client = match query_client(request.mode, &service) {
            Ok(client) => client,
            Err(error) => return render_setup_error(json, error),
        };
        let search = PreparedSearch {
            request,
            recipe,
            client,
        };
        search.run(path, json).await
    }

    /// Converts process arguments to the engine's retrieval and filtering contract.
    fn request(self) -> SearchRequest {
        let mode = match self.mode {
            SearchModeArg::Keyword => SearchMode::Keyword,
            SearchModeArg::AdvancedFts => SearchMode::AdvancedFts,
            SearchModeArg::Semantic => SearchMode::Semantic,
            SearchModeArg::Hybrid => SearchMode::Hybrid,
        };
        let filters = thread_filters(
            self.repositories,
            self.kind,
            self.state,
            self.sort,
            self.limit,
            self.offset,
        );
        SearchRequest {
            query: self.query,
            mode,
            filters,
            allow_keyword_fallback: self.keyword_fallback,
        }
    }
}

/// Reports fallback policy that cannot apply to the selected local-only retrieval mode.
fn invalid_fallback(json: OutputMode) -> ExitCode {
    render_error_with_status(
        json,
        "search",
        "search_fallback_mode_invalid",
        "--keyword-fallback requires --mode semantic or --mode hybrid",
        ExitCode::from(2),
    )
}

/// Prepares a client only for modes that must acquire a query vector.
fn query_client(
    mode: SearchMode,
    service: &EmbeddingServiceConfig,
) -> Result<Option<EmbeddingClient>, EmbeddingSetupError> {
    if requires_embedding(mode) {
        service.client().map(Some)
    } else {
        Ok(None)
    }
}

/// Identifies retrieval modes whose query representation comes from the embedding service.
fn requires_embedding(mode: SearchMode) -> bool {
    matches!(mode, SearchMode::Semantic | SearchMode::Hybrid)
}

/// Preserves usage versus transport initialization failure in search's process exit status.
fn render_setup_error(json: OutputMode, error: EmbeddingSetupError) -> ExitCode {
    let status = match &error {
        EmbeddingSetupError::Configuration(_) => ExitCode::from(2),
        EmbeddingSetupError::Client(_) => ExitCode::FAILURE,
    };
    render_error_with_status(json, "search", error.code(), &error.to_string(), status)
}

/// Validated retrieval inputs and the optional query-vector transport they require.
///
/// This command-local owner keeps service preparation outside archive execution. It borrows no
/// open archive, so setup failures cannot leave a connection pool waiting to be closed.
struct PreparedSearch {
    /// Query, repository filters, pagination, and fallback policy passed to the engine.
    request: SearchRequest,
    /// Stored document recipe used to select compatible vectors.
    recipe: DocumentRecipe,
    /// Query embedding client; absent for keyword and advanced FTS retrieval.
    client: Option<EmbeddingClient>,
}

impl PreparedSearch {
    /// Opens read-only access, retrieves with cancellation, then closes before rendering.
    async fn run(self, path: &std::path::Path, json: OutputMode) -> ExitCode {
        let interruption = CommandInterruption::new();
        let archive = match Archive::open_read_only(path).await {
            Ok(archive) => archive,
            Err(error) => return render_store_error(json, "search", error),
        };
        let result = retrieve_threads(
            &archive,
            &self.request,
            self.recipe,
            self.client.as_ref(),
            interruption.cancellation(),
        )
        .await;
        archive.close().await;
        match result {
            Ok(page) => render_search_page(json, &page),
            Err(error) => render_engine_error(json, "search", error),
        }
    }
}
