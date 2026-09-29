//! Repository thread enumeration workflow.
//!
//! Scan repository discussions and record progress before detailed family acquisition. The result
//! distinguishes a completed repository scan from one interrupted after a checkpoint.

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use forgesync_core::content::Repository;
use forgesync_core::coverage::{EvidenceFamily, Failure, FailureKind};
use forgesync_core::observation::{CollectionCompleteness, Observation, SourceClock};
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_github::error::{ApiFailureKind, GitHubError};
use forgesync_github::resources::{
    ThreadListState, fetch_repository, fetch_thread_page_in_scope, thread_list_url_in_scope,
};
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use forgesync_store::enumeration::{RepositoryThreadScan, RepositoryThreadScanStatus};
use forgesync_store::error::StoreError;
use forgesync_store::leases::ArchiveLeaseToken;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::EngineError;
use crate::reference::RepositorySelector;

/// Result of enumerating all currently visible issues and pull requests in one repository.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ThreadEnumerationReport {
    /// Latest repository identity returned by GitHub, including any renamed path.
    pub repository: Repository,
    /// Durable scan state and last committed pagination cursor.
    pub scan: RepositoryThreadScan,
    /// True when caller cancellation stopped page acquisition.
    pub interrupted: bool,
}

pub(crate) struct ThreadScanContext {
    pub repository: Repository,
    pub sequence: forgesync_core::identity::ObservationSequence,
    pub started_at: UtcTimestamp,
    pub state: ThreadListState,
    pub since: Option<UtcTimestamp>,
}

/// Fetches and durably applies every page of repository issues and pull requests.
///
/// The acquisition sequence is reserved before the first request. Each page's thread rows are
/// committed before its cursor advances, so a failed later page leaves earlier content usable and
/// the stored scan explicitly incomplete.
pub async fn enumerate_repository_threads(
    archive: &Archive,
    client: &GitHubClient,
    selector: &RepositorySelector,
    cancellation: &CancellationToken,
) -> Result<ThreadEnumerationReport, EngineError> {
    enumerate_repository_threads_in_scope(
        archive,
        client,
        selector,
        ThreadListState::All,
        None,
        None,
        cancellation,
    )
    .await
}

/// Fetches and applies one selected state scope while enforcing an archive lease fence.
pub async fn enumerate_repository_threads_in_scope(
    archive: &Archive,
    client: &GitHubClient,
    selector: &RepositorySelector,
    state: ThreadListState,
    since: Option<UtcTimestamp>,
    lease: Option<&ArchiveLeaseToken>,
    cancellation: &CancellationToken,
) -> Result<ThreadEnumerationReport, EngineError> {
    let started_at = now_utc()?;
    let sequence = match lease {
        Some(lease) => {
            archive
                .reserve_observation_sequence_fenced(started_at, lease)
                .await?
        }
        None => archive.reserve_observation_sequence(started_at).await?,
    };
    let repository = fetch_repository(
        client,
        selector.host(),
        selector.owner(),
        selector.name(),
        cancellation,
    )
    .await?;
    match lease {
        Some(lease) => {
            archive.upsert_repository_fenced(&repository, lease).await?;
        }
        None => {
            archive.upsert_repository(&repository).await?;
        }
    }

    enumerate_repository_thread_pages(
        archive,
        client,
        ThreadScanContext {
            repository,
            sequence,
            started_at,
            state,
            since,
        },
        lease,
        cancellation,
    )
    .await
}

pub(crate) async fn enumerate_repository_thread_pages(
    archive: &Archive,
    client: &GitHubClient,
    context: ThreadScanContext,
    lease: Option<&ArchiveLeaseToken>,
    cancellation: &CancellationToken,
) -> Result<ThreadEnumerationReport, EngineError> {
    let ThreadScanContext {
        repository,
        sequence,
        started_at,
        state,
        since,
    } = context;
    let first_page = thread_list_url_in_scope(client, &repository, state, since)?;
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

    let mut page_url = None;
    let mut visited_pages = HashSet::new();
    loop {
        let requested_url = page_url.as_ref().unwrap_or(&first_page);
        if !visited_pages.insert(requested_url.as_str().to_owned()) {
            return incomplete_report(
                archive,
                repository,
                sequence,
                Failure {
                    kind: FailureKind::ProviderResponse,
                    message: "GitHub pagination returned a repeated page URL".to_owned(),
                },
                false,
                lease,
            )
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
            Err(error) => {
                let interrupted = error == GitHubError::Cancelled;
                let failure = (!interrupted).then(|| github_failure(&error));
                return finish_report(
                    archive,
                    repository,
                    sequence,
                    RepositoryThreadScanStatus::Incomplete,
                    failure,
                    interrupted,
                    lease,
                )
                .await;
            }
        };

        let page_thread_count = page.discussions.len() as u64;
        for discussion in page.discussions {
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
            if applied.is_err() {
                return incomplete_report(
                    archive,
                    repository,
                    sequence,
                    Failure {
                        kind: FailureKind::Archive,
                        message: "archive could not commit a repository thread page".to_owned(),
                    },
                    false,
                    lease,
                )
                .await;
            }
        }

        page_url = page.next_page;
        let next_page_url = page_url.as_ref().map(|url| url.as_str());
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

        if page_url.is_none() {
            return finish_report(
                archive,
                repository,
                sequence,
                RepositoryThreadScanStatus::Complete,
                None,
                false,
                lease,
            )
            .await;
        }
    }
}

async fn incomplete_report(
    archive: &Archive,
    repository: Repository,
    sequence: forgesync_core::identity::ObservationSequence,
    failure: Failure,
    interrupted: bool,
    lease: Option<&ArchiveLeaseToken>,
) -> Result<ThreadEnumerationReport, EngineError> {
    finish_report(
        archive,
        repository,
        sequence,
        RepositoryThreadScanStatus::Incomplete,
        Some(failure),
        interrupted,
        lease,
    )
    .await
}

async fn finish_report(
    archive: &Archive,
    repository: Repository,
    sequence: forgesync_core::identity::ObservationSequence,
    status: RepositoryThreadScanStatus,
    failure: Option<Failure>,
    interrupted: bool,
    lease: Option<&ArchiveLeaseToken>,
) -> Result<ThreadEnumerationReport, EngineError> {
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

pub(super) fn github_failure(error: &GitHubError) -> Failure {
    let kind = match error {
        GitHubError::Api {
            kind: ApiFailureKind::AuthenticationRequired,
            ..
        } => FailureKind::Authentication,
        GitHubError::Api {
            kind: ApiFailureKind::PermissionDenied,
            ..
        } => FailureKind::PermissionDenied,
        GitHubError::Deferred { .. }
        | GitHubError::Api {
            kind: ApiFailureKind::RateLimited,
            ..
        } => FailureKind::RateLimited,
        GitHubError::Network | GitHubError::Timeout => FailureKind::Network,
        GitHubError::InvalidProviderData => FailureKind::InvalidData,
        GitHubError::Cancelled
        | GitHubError::Api { .. }
        | GitHubError::UntrustedOrigin
        | GitHubError::InvalidPaginationLink
        | GitHubError::RedirectRejected
        | GitHubError::ResponseTooLarge
        | GitHubError::InvalidJson
        | GitHubError::GraphqlErrors { .. }
        | GitHubError::ConcurrencyUnavailable
        | GitHubError::InvalidApiBaseUrl
        | GitHubError::InvalidConfiguration
        | GitHubError::ClientInitialization => FailureKind::ProviderResponse,
    };
    Failure {
        kind,
        message: error.to_string(),
    }
}

pub(super) fn now_utc() -> Result<UtcTimestamp, EngineError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StoreError::ClockOutOfRange)?;
    let microseconds =
        i64::try_from(elapsed.as_micros()).map_err(|_| StoreError::ClockOutOfRange)?;
    UtcTimestamp::from_unix_microseconds(microseconds)
        .map_err(StoreError::InvalidCreatedAt)
        .map_err(Into::into)
}
