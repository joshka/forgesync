//! # Apply repository scan pages before advancing coverage
//!
//! Page traversal records the initial URL, rejects pagination cycles, and acquires scoped thread
//! pages. `ScanPersistence` owns the durable begin, apply, and cursor phases for the reserved scan.
//! Parent observations commit before the cursor advances; terminal coverage is complete only after
//! the final page has been durably recorded.
//!
//! `enumeration` reserves the acquisition and resolves the repository. Sync `thread_job` supplies
//! a writer lease for coordinated acquisition. Provider failures and cancellation preserve partial
//! scan state; failed page writes retain prior durable rows without claiming complete coverage.
//!
//! This module acquires parent discussions only, leaving child evidence to sync family collectors.

use std::collections::HashSet;

use forgesync_core::coverage::{Failure, FailureKind};
use forgesync_github::resources::{fetch_thread_page_in_scope, thread_list_url_in_scope};
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use forgesync_store::leases::ArchiveLeaseToken;
use tokio_util::sync::CancellationToken;

use crate::enumeration::scan_outcome::ScanOutcome;
use crate::enumeration::scan_persistence::ScanPersistence;
use crate::enumeration::{ThreadEnumerationReport, ThreadScanContext};
use crate::error::EngineError;

/// Commits each acquired page before advancing the durable scan cursor.
pub(crate) async fn enumerate_repository_thread_pages(
    archive: &Archive,
    client: &GitHubClient,
    context: ThreadScanContext,
    lease: Option<&ArchiveLeaseToken>,
    cancellation: &CancellationToken,
) -> Result<ThreadEnumerationReport, EngineError> {
    let persistence = ScanPersistence {
        archive,
        context: &context,
        lease,
    };
    let first_page =
        thread_list_url_in_scope(client, &context.repository, context.state, context.since)?;
    persistence.begin(&first_page).await?;

    let mut page_url = None;
    let mut visited_pages = HashSet::new();
    loop {
        let requested_url = page_url.as_ref().unwrap_or(&first_page);
        if !visited_pages.insert(requested_url.as_str().to_owned()) {
            return persistence
                .finish(ScanOutcome::Failed(Failure {
                    kind: FailureKind::ProviderResponse,
                    message: "GitHub pagination returned a repeated page URL".to_owned(),
                }))
                .await;
        }

        let page = match fetch_thread_page_in_scope(
            client,
            &context.repository,
            page_url.as_ref(),
            context.state,
            context.since,
            cancellation,
        )
        .await
        {
            Ok(page) => page,
            Err(error) => return persistence.finish(ScanOutcome::from(error)).await,
        };

        let page_thread_count = page.discussions.len() as u64;
        if persistence.apply(page.discussions).await.is_err() {
            return persistence
                .finish(ScanOutcome::Failed(Failure {
                    kind: FailureKind::Archive,
                    message: "archive could not commit a repository thread page".to_owned(),
                }))
                .await;
        }

        page_url = page.next_page;
        let next_page_url = page_url.as_ref().map(|url| url.as_str());
        persistence
            .record_page(page_thread_count, next_page_url)
            .await?;

        if page_url.is_none() {
            return persistence.finish(ScanOutcome::Complete).await;
        }
    }
}
