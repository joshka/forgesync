//! Search the local archive.
//!
//! Keyword and advanced FTS reads stay offline. Semantic and hybrid modes send the query text to
//! the configured embedding service and compare it with stored discussion vectors. No mode fetches
//! GitHub data or writes to the archive.

use std::path::Path;

use clap::{ArgAction, Args};
use forgesync_engine::search::{SearchMode, SearchRequest, retrieve_threads};
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;

use super::with_archive;
use crate::command::thread_filters::ThreadFilterArgs;
use crate::command::values::SearchModeArg;
use crate::config::ForgesyncConfig;
use crate::error::{CliError, Exit};
use crate::output::Output;
use crate::reports::threads::render_search_page;

/// Search archived discussions with local keyword or optional semantic ranking.
#[derive(Clone, Debug, Args)]
pub struct SearchArgs {
    /// Search text sent to the selected local or semantic retrieval mode.
    pub query: String,
    #[command(flatten)]
    pub filters: ThreadFilterArgs,
    /// Choose keyword, semantic, hybrid, or explicit FTS5 retrieval.
    #[arg(long, value_enum, default_value_t = SearchModeArg::Keyword)]
    pub mode: SearchModeArg,
    /// Return keyword results if semantic retrieval has no compatible data or fails.
    #[arg(long, action = ArgAction::SetTrue)]
    pub keyword_fallback: bool,
}

impl SearchArgs {
    /// Rejects inapplicable fallback, prepares a query client for semantic modes, then searches
    /// read-only.
    pub async fn run(
        self,
        path: &Path,
        output: Output,
        config: ForgesyncConfig,
        cancellation: &CancellationToken,
    ) -> Result<Exit, CliError> {
        let request = self.request();
        let semantic = matches!(request.mode, SearchMode::Semantic | SearchMode::Hybrid);
        if request.allow_keyword_fallback && !semantic {
            return Err(CliError::InvalidArguments {
                code: "search_fallback_mode_invalid",
                message: "--keyword-fallback requires --mode semantic or --mode hybrid",
            });
        }
        let client = semantic.then(|| config.embeddings.client()).transpose()?;
        let page = with_archive(Archive::open_read_only(path), async |archive| {
            retrieve_threads(
                archive,
                &request,
                config.documents.recipe,
                client.as_ref(),
                cancellation,
            )
            .await
        })
        .await?;
        Ok(render_search_page(output, &page))
    }

    /// Converts parsed arguments into the engine request.
    fn request(self) -> SearchRequest {
        SearchRequest {
            query: self.query,
            mode: match self.mode {
                SearchModeArg::Keyword => SearchMode::Keyword,
                SearchModeArg::AdvancedFts => SearchMode::AdvancedFts,
                SearchModeArg::Semantic => SearchMode::Semantic,
                SearchModeArg::Hybrid => SearchMode::Hybrid,
            },
            filters: self.filters.into_filters(),
            allow_keyword_fallback: self.keyword_fallback,
        }
    }
}
