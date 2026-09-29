//! # Assemble a GitHub acquisition workflow outside the CLI
//!
//! This example shows what a caller must supply to run the engine's repository sync directly. It
//! wires an existing writable archive, a repository selector, a host-specific GitHub client, a sync
//! request, and a cancellation token into `sync_repositories`. Its purpose is to make the
//! application boundary concrete for another executable or integration that wants engine behavior
//! with its own startup and presentation policy.
//!
//! ## What this demonstrates
//!
//! The caller reads `GITHUB_TOKEN` and creates the authenticated client. The engine receives that
//! client in a map keyed by `GitHubHost`, so provider selection is explicit rather than discovered
//! from the process environment. This example accepts GitHub.com repositories only and fixes the
//! API base URL to `https://api.github.com/`; enterprise endpoint and credential resolution belong to a more
//! complete caller such as the CLI.
//!
//! `Archive::open_read_write` requires an existing archive. `SyncRequest` selects one repository
//! with the default thread scope: open threads and the durable closed-thread sweep. Comments,
//! reviews, and review threads are requested as independent evidence families, with
//! pull-request-specific work applied where relevant. `parent_run: None` starts this as a top-level
//! attempt rather than a child of another workflow. The engine owns acquisition, failure isolation,
//! and durable run reporting; the store owns observation ordering and the completeness rules for
//! committing child membership.
//!
//! The token makes cancellation an explicit input. This minimal program never cancels it or
//! installs a signal handler, and passing `None` for progress means it waits for the terminal
//! report. A richer caller can cancel during shutdown and consume progress while the same workflow
//! runs.
//!
//! ## Run it
//!
//! Create the archive explicitly first, then supply a token through your shell's credential setup
//! and run the example. The token must have access to the selected repository:
//!
//! ```sh
//! cargo run -p forgesync-cli -- --archive ./archive.sqlite archive init
//! cargo run -p forgesync-engine --example sync_repository -- ./archive.sqlite ratatui/ratatui
//! ```
//!
//! The example reads `GITHUB_TOKEN` from the environment; it does not load the CLI config or invoke
//! a credential helper. It performs GitHub reads and local archive writes, including a durable run
//! and acquired observations. It does not generate embeddings or cluster analysis.
//!
//! ## Interpret the result
//!
//! The printed line includes the run ID, structured outcome, and counts of observed threads,
//! comments, and pages. It is a compact demonstration, not a complete failure report: inspect the
//! `SyncReport` and run detail when building a caller that needs per-job diagnostics. A returned
//! report can describe partial success, so the absence of a propagated error alone does not
//! establish complete coverage. Compare `offline_search` for a read-only consumer of the evidence
//! this workflow stores.

use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::path::PathBuf;

use forgesync_core::identity::GitHubHost;
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::sync::{SyncRequest, SyncThreadScope, sync_repositories};
use forgesync_github::token::GitHubToken;
use forgesync_github::transport::{GitHubClient, GitHubClientConfig};
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;
use url::Url;

/// Acquires one repository's selected evidence and prints its terminal run summary.
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
