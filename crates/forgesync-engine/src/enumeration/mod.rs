//! # Enumerate repository discussion identities
//!
//! Enumeration asks the GitHub adapter for repository threads and records scan coverage in the
//! archive. `ThreadEnumerationReport` tells the caller what was visited and whether the scan
//! completed.
//!
//! This is a discovery workflow, not a full sync of each discussion or child family. Scope-aware
//! variants allow the sync coordinator to ask for only the identities it needs. The store persists
//! scan state so a later run can distinguish an empty complete repository from an interrupted
//! scan.
//!
//! `scan` owns provider traversal and pagination-cycle detection. `scan_persistence` owns the
//! reserved archive write phases, while `scan_outcome` maps acquisition completion, cancellation,
//! and failure to durable coverage and report diagnostics. Cursor advancement follows parent
//! observation commits; terminal coverage follows the recorded terminal page.

use std::time::{SystemTime, UNIX_EPOCH};

use forgesync_core::content::Repository;
use forgesync_core::coverage::{Failure, FailureKind};
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_github::error::{ApiFailureKind, GitHubError};
use forgesync_github::resources::{ThreadListState, fetch_repository};
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use forgesync_store::enumeration::RepositoryThreadScan;
use forgesync_store::error::StoreError;
use forgesync_store::leases::ArchiveLeaseToken;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::error::EngineError;
use crate::reference::RepositorySelector;

mod scan;
mod scan_outcome;
mod scan_persistence;
pub(crate) use scan::enumerate_repository_thread_pages;

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

/// Reserved repository acquisition identity, source scope, and observation coordinates.
///
/// The same sequence/time applies to all parent observations and scan writes. State and cutoff
/// constrain provider traversal; they do not change the ordering of canonical observation writes.
#[derive(Clone)]
pub(crate) struct ThreadScanContext {
    /// Provider-resolved repository, including its current renamed path.
    pub repository: Repository,
    /// Acquisition order reserved before provider requests begin.
    pub sequence: forgesync_core::identity::ObservationSequence,
    /// Local acquisition time shared by the scan's parent observations.
    pub started_at: UtcTimestamp,
    /// Provider list scope selected by the enumeration coordinator.
    pub state: ThreadListState,
    /// Optional provider update cutoff for an incremental scan.
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

/// Commits each acquired page before advancing the durable scan cursor.
/// Converts a typed provider error to structured family failure evidence.
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

/// Validates local acquisition time before recording a scan.
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
