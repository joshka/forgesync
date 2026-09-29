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

use forgesync_core::coverage::{EvidenceFamily, Failure, FailureKind};
use forgesync_core::observation::{CollectionCompleteness, Observation, SourceClock};
use forgesync_github::error::GitHubError;
use forgesync_github::resources::{fetch_thread_page_in_scope, thread_list_url_in_scope};
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use forgesync_store::enumeration::RepositoryThreadScanStatus;
use forgesync_store::error::StoreError;
use forgesync_store::leases::ArchiveLeaseToken;
use tokio_util::sync::CancellationToken;

use super::{ThreadEnumerationReport, ThreadScanContext, github_failure, now_utc};
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
    let ThreadScanContext {
        repository,
        sequence: _,
        started_at: _,
        state,
        since,
    } = context.clone();
    let first_page = thread_list_url_in_scope(client, &repository, state, since)?;
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
            &repository,
            page_url.as_ref(),
            state,
            since,
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

/// Durable write phases belonging to one reserved repository scan.
///
/// The page cursor advances only after all thread observations have committed. A failed page can
/// leave usable earlier rows while its scan remains incomplete; it cannot establish full coverage.
struct ScanPersistence<'a> {
    archive: &'a Archive,
    context: &'a ThreadScanContext,
    lease: Option<&'a ArchiveLeaseToken>,
}
impl ScanPersistence<'_> {
    /// Records the first requested page before any provider I/O.
    async fn begin(&self, first_page: &url::Url) -> Result<(), EngineError> {
        let ThreadScanContext {
            repository,
            sequence,
            started_at,
            state: _,
            since: _,
        } = self.context.clone();
        let archive = self.archive;
        let lease = self.lease;
        match lease {
            Some(lease) => {
                archive
                    .begin_repository_thread_scan_fenced(
                        &repository.id,
                        sequence,
                        started_at,
                        first_page.as_str(),
                        lease,
                    )
                    .await?;
            }
            None => {
                archive
                    .begin_repository_thread_scan(
                        &repository.id,
                        sequence,
                        started_at,
                        first_page.as_str(),
                    )
                    .await?;
            }
        }

        Ok(())
    }
    /// Commits every parent observation before the caller may advance the durable cursor.
    async fn apply(
        &self,
        discussions: Vec<forgesync_core::content::Discussion>,
    ) -> Result<(), EngineError> {
        let ThreadScanContext {
            repository: _,
            sequence,
            started_at,
            state: _,
            since: _,
        } = self.context.clone();
        let archive = self.archive;
        let lease = self.lease;
        for discussion in discussions {
            let source_clock = SourceClock::Valid(discussion.updated_at);
            let observation = Observation::new(
                EvidenceFamily::Threads,
                discussion,
                source_clock,
                started_at,
                sequence,
                CollectionCompleteness::Complete,
            );
            let applied = match lease {
                Some(lease) => {
                    archive
                        .apply_thread_observation_fenced(&observation, lease)
                        .await
                }
                None => archive.apply_thread_observation(&observation).await,
            };
            applied?;
        }
        Ok(())
    }
    /// Advances the cursor after durable page application, including an empty terminal page.
    async fn record_page(
        &self,
        page_thread_count: u64,
        next_page_url: Option<&str>,
    ) -> Result<(), EngineError> {
        let ThreadScanContext {
            repository,
            sequence,
            started_at: _,
            state: _,
            since: _,
        } = self.context.clone();
        let archive = self.archive;
        let lease = self.lease;
        match lease {
            Some(lease) => {
                archive
                    .record_repository_thread_scan_page_fenced(
                        &repository.id,
                        sequence,
                        page_thread_count,
                        next_page_url,
                        now_utc()?,
                        lease,
                    )
                    .await?;
            }
            None => {
                archive
                    .record_repository_thread_scan_page(
                        &repository.id,
                        sequence,
                        page_thread_count,
                        next_page_url,
                        now_utc()?,
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

/// Terminal state of a reserved scan, with failure evidence only when acquisition failed.
enum ScanOutcome {
    Complete,
    Interrupted,
    Failed(Failure),
}
impl From<GitHubError> for ScanOutcome {
    /// Treats provider cancellation as interruption; other errors become safe durable scan
    /// failures.
    fn from(error: GitHubError) -> Self {
        match error {
            GitHubError::Cancelled => Self::Interrupted,
            error => Self::Failed(github_failure(&error)),
        }
    }
}

impl ScanPersistence<'_> {
    /// Records terminal coverage after page writes, retaining failure or interruption distinctly.
    async fn finish(&self, outcome: ScanOutcome) -> Result<ThreadEnumerationReport, EngineError> {
        let archive = self.archive;
        let lease = self.lease;
        let repository = self.context.repository.clone();
        let sequence = self.context.sequence;
        let status = match outcome {
            ScanOutcome::Complete => RepositoryThreadScanStatus::Complete,
            ScanOutcome::Interrupted | ScanOutcome::Failed(_) => {
                RepositoryThreadScanStatus::Incomplete
            }
        };
        let interrupted = matches!(outcome, ScanOutcome::Interrupted);
        let failure = match outcome {
            ScanOutcome::Failed(failure) => Some(failure),
            ScanOutcome::Complete | ScanOutcome::Interrupted => None,
        };
        match lease {
            Some(lease) => {
                archive
                    .finish_repository_thread_scan_fenced(
                        &repository.id,
                        sequence,
                        status,
                        now_utc()?,
                        failure.as_ref(),
                        lease,
                    )
                    .await?;
            }
            None => {
                archive
                    .finish_repository_thread_scan(
                        &repository.id,
                        sequence,
                        status,
                        now_utc()?,
                        failure.as_ref(),
                    )
                    .await?;
            }
        }
        let scan = archive
            .repository_thread_scan(&repository.id)
            .await?
            .ok_or(StoreError::RepositoryThreadScanMissing)?;
        Ok(ThreadEnumerationReport {
            repository,
            scan,
            interrupted,
        })
    }
}
