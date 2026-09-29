use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::path::PathBuf;

use forgesync_core::identity::GitHubHost;
use forgesync_engine::{RepositorySelector, SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_github::token::GitHubToken;
use forgesync_github::transport::{GitHubClient, GitHubClientConfig};
use forgesync_store::Archive;
use tokio_util::sync::CancellationToken;
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let archive_path = PathBuf::from(arguments.next().ok_or(
        "usage: cargo run -p forgesync-engine --example sync_repository -- ARCHIVE OWNER/REPO",
    )?);
    let repository = arguments
        .next()
        .ok_or(
            "usage: cargo run -p forgesync-engine --example sync_repository -- ARCHIVE OWNER/REPO",
        )?
        .to_string_lossy()
        .parse::<RepositorySelector>()?;
    if repository.host().as_str() != "github.com" {
        return Err(std::io::Error::other("this example supports GitHub.com only").into());
    }
    let token = GitHubToken::new(env::var("GITHUB_TOKEN")?)?;
    let client = GitHubClient::new(
        GitHubClientConfig::new(Url::parse("https://api.github.com/")?),
        Some(token),
    )?;

    let mut clients = HashMap::<GitHubHost, GitHubClient>::new();
    clients.insert(repository.host().clone(), client);

    let archive = Archive::open_read_write(archive_path).await?;
    let request = SyncRequest {
        repositories: vec![repository],
        all: false,
        scope: SyncThreadScope::Default,
        include_comments: true,
        include_reviews: true,
        include_review_threads: true,
        parent_run: None,
    };
    let cancellation = CancellationToken::new();
    let report_future = sync_repositories(&archive, &clients, &request, &cancellation, None);
    let report = report_future.await?;

    println!(
        "run {}: {:?}; {} threads and {} comments from {} pages",
        report.run.id.get(),
        report.outcome,
        report.threads_seen,
        report.comments_seen,
        report.pages_completed
    );

    archive.close().await;
    Ok(())
}
