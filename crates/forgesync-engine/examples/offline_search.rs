//! # Read an archive through the engine API
//!
//! This example is a small integration recipe for an application that wants to search an existing
//! Forgesync archive. It shows the complete caller path: open a read-only archive, describe a
//! keyword query with `SearchRequest`, call `search_threads`, display the returned discussions, and
//! close the archive. Use it when learning how to consume the engine without adopting the CLI's
//! configuration, argument model, or output envelopes.
//!
//! ## What this demonstrates
//!
//! The caller owns archive lifecycle and query selection. `Archive::open_read_only` makes the
//! intended access mode explicit; the engine interprets the search request and the store handles
//! local query execution. `ThreadFilters::default()` supplies the normal unqualified filter set.
//! Choosing `SearchMode::Keyword` makes this example independent of embedding-model configuration
//! and previously generated vectors. The returned page contains domain-facing hits rather than SQL
//! rows.
//!
//! The printed repository, discussion number, and title are deliberately a small projection of each
//! hit. An application can instead use the full result for navigation, provenance, or another
//! output format. This example reads one page; it does not demonstrate a pagination UI or every
//! search mode. See the engine's `search` module for mode, ranking, and fallback contracts.
//!
//! ## Run it
//!
//! Use an existing archive with discussions already acquired. Opening a missing archive fails; this
//! program does not initialize or refresh it. Pass the whole query as one shell argument:
//!
//! ```sh
//! cargo run -p forgesync-engine --example offline_search -- ./archive.sqlite 'terminal resize'
//! ```
//!
//! The two positional arguments are the archive path and query text. No credentials or network
//! service are required. A successful run prints one line per hit; no printed lines can simply mean
//! that the query found no matches. Errors propagate to the process rather than being rendered with
//! the CLI's JSON or diagnostic conventions.
//!
//! ## Where to extend it
//!
//! Start here for a local archive viewer or an embedded search tool. Add explicit filters and
//! result presentation at the caller, and keep search policy in the engine. Compare
//! `sync_repository` for the separate workflow that acquires evidence into an archive before these
//! reads can find it.

use std::env;
use std::error::Error;
use std::path::PathBuf;

use forgesync_engine::inspect::ThreadFilters;
use forgesync_engine::search::{SearchMode, SearchRequest, search_threads};
use forgesync_store::archive::Archive;

/// Reads one keyword-result page from the selected archive and prints navigation labels.
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let archive_path =
        PathBuf::from(arguments.next().ok_or(
            "usage: cargo run -p forgesync-engine --example offline_search -- ARCHIVE QUERY",
        )?);
    let query = arguments
        .next()
        .ok_or("usage: cargo run -p forgesync-engine --example offline_search -- ARCHIVE QUERY")?
        .to_string_lossy()
        .into_owned();

    let archive = Archive::open_read_only(archive_path).await?;
    let request = SearchRequest {
        query,
        mode: SearchMode::Keyword,
        filters: ThreadFilters::default(),
        allow_keyword_fallback: false,
    };
    let page = search_threads(&archive, &request).await?;

    for item in page.items {
        println!(
            "{}#{}: {}",
            item.repository.full_name,
            item.discussion.id.number().get(),
            item.discussion.title
        );
    }

    archive.close().await;
    Ok(())
}
