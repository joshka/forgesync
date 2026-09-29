use std::env;
use std::error::Error;
use std::path::PathBuf;

use forgesync_engine::inspect::ThreadFilters;
use forgesync_engine::search::{SearchMode, SearchRequest, search_threads};
use forgesync_store::archive::Archive;

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
