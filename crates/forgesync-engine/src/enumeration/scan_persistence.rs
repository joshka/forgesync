//! # Persist one reserved repository scan
//!
//! `ScanPersistence` binds an opened archive, reserved scan context, and optional writer fence.
//! `begin` records the initial cursor before provider I/O. `apply` commits parent observations;
//! `record_page` advances the cursor only after those writes succeed.
//!
//! `finish` persists terminal coverage before reading the durable report. A complete outcome
//! requires the traversal to have recorded its terminal page. Failed or cancelled scans retain
//! earlier observations and incomplete coverage. This module performs no provider requests.
//!
//! The optional fence selects existing store operations; it does not change ordering or content
//! semantics. Each observation and cursor write retains its independent transaction boundary.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::observation::{CollectionCompleteness, Observation, SourceClock};
use forgesync_store::archive::Archive;
use forgesync_store::error::StoreError;
use forgesync_store::leases::ArchiveLeaseToken;

use crate::clock::now_utc;
use crate::enumeration::scan_outcome::ScanOutcome;
use crate::enumeration::{ThreadEnumerationReport, ThreadScanContext};
use crate::error::EngineError;

/// Durable write phases belonging to one reserved repository scan.
///
/// The page cursor advances only after all thread observations have committed. A failed page can
/// leave usable earlier rows while its scan remains incomplete; it cannot establish full coverage.
pub struct ScanPersistence<'a> {
    /// Open archive receiving observations and scan state.
    pub archive: &'a Archive,
    /// Reserved identity, acquisition sequence, and observation time.
    pub context: &'a ThreadScanContext,
    /// Optional writer ownership checked independently by every mutation.
    pub lease: Option<&'a ArchiveLeaseToken>,
}
impl ScanPersistence<'_> {
    /// Records the first requested page before any provider I/O.
    pub async fn begin(&self, first_page: &url::Url) -> Result<(), EngineError> {
        let context = self.context;
        let archive = self.archive;
        let lease = self.lease;
        match lease {
            Some(lease) => {
                archive
                    .begin_repository_thread_scan_fenced(
                        &context.repository.id,
                        context.sequence,
                        context.started_at,
                        first_page.as_str(),
                        lease,
                    )
                    .await?;
            }
            None => {
                archive
                    .begin_repository_thread_scan(
                        &context.repository.id,
                        context.sequence,
                        context.started_at,
                        first_page.as_str(),
                    )
                    .await?;
            }
        }

        Ok(())
    }
    /// Commits every parent observation before the caller may advance the durable cursor.
    pub async fn apply(
        &self,
        discussions: Vec<forgesync_core::content::Discussion>,
    ) -> Result<(), EngineError> {
        let context = self.context;
        let archive = self.archive;
        let lease = self.lease;
        for discussion in discussions {
            let source_clock = SourceClock::Valid(discussion.updated_at);
            let observation = Observation::new(
                EvidenceFamily::Threads,
                discussion,
                source_clock,
                context.started_at,
                context.sequence,
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
    pub async fn record_page(
        &self,
        page_thread_count: u64,
        next_page_url: Option<&str>,
    ) -> Result<(), EngineError> {
        let context = self.context;
        let archive = self.archive;
        let lease = self.lease;
        match lease {
            Some(lease) => {
                archive
                    .record_repository_thread_scan_page_fenced(
                        &context.repository.id,
                        context.sequence,
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
                        &context.repository.id,
                        context.sequence,
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

impl ScanPersistence<'_> {
    /// Records terminal coverage after page writes, retaining failure or interruption distinctly.
    pub async fn finish(
        &self,
        outcome: ScanOutcome,
    ) -> Result<ThreadEnumerationReport, EngineError> {
        let archive = self.archive;
        let context = self.context;
        self.record_outcome(&outcome).await?;
        let scan = archive
            .repository_thread_scan(&context.repository.id)
            .await?
            .ok_or(StoreError::RepositoryThreadScanMissing)?;
        Ok(ThreadEnumerationReport {
            repository: context.repository.clone(),
            scan,
            interrupted: outcome.interrupted(),
        })
    }

    /// Writes terminal coverage with failure evidence only for an unsuccessful provider attempt.
    async fn record_outcome(&self, outcome: &ScanOutcome) -> Result<(), EngineError> {
        let context = self.context;
        match self.lease {
            Some(lease) => {
                self.archive
                    .finish_repository_thread_scan_fenced(
                        &context.repository.id,
                        context.sequence,
                        outcome.status(),
                        now_utc()?,
                        outcome.failure(),
                        lease,
                    )
                    .await?
            }
            None => {
                self.archive
                    .finish_repository_thread_scan(
                        &context.repository.id,
                        context.sequence,
                        outcome.status(),
                        now_utc()?,
                        outcome.failure(),
                    )
                    .await?
            }
        }
        Ok(())
    }
}
