//! # Enumerate repository discussion identities
//!
//! Enumeration asks the GitHub adapter for repository threads and records scan coverage in the
//! archive. `ThreadEnumerationReport` tells the caller what was visited and whether the scan
//! completed.
//!
//! This is a discovery workflow, not a full sync of each discussion or child family. The private
//! page executor lets sync supply its reserved scope and writer authority. The store persists
//! scan state so a later run can distinguish an empty complete repository from an interrupted
//! scan.
//!
//! `scan` owns provider traversal and pagination-cycle detection. `scan_persistence` owns the
//! reserved archive write phases, while `scan_outcome` maps acquisition completion, cancellation,
//! and failure to durable coverage and report diagnostics. Cursor advancement follows parent
//! observation commits; terminal coverage follows the recorded terminal page.

use forgesync_core::content::Repository;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_github::resources::{ThreadListState, fetch_repository};
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use forgesync_store::enumeration::RepositoryThreadScan;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::clock::now_utc;
use crate::error::EngineError;
use crate::reference::RepositorySelector;

mod scan;
mod scan_outcome;
mod scan_persistence;
// Sync supplies its already reserved sequence and writer fence to the same page executor.
// Keep this bridge crate-only: public callers use the coordinator that prepares those facts.
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
///
/// This crate-only seam lets sync reuse page acquisition after preparing its own ledger and lease.
/// Public enumeration callers use the coordinator, which resolves the repository and reserves
/// acquisition order. Publishing this context would offer a second entry point requiring callers
/// to coordinate those invariants manually; its fields are not an independently validated request.
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
///
/// This discovery entry point acquires all thread states without enforcing a writer lease. Sync
/// supplies its own reserved state/cutoff and fence through the private page executor instead.
/// Repository resolution and acquisition order happen before page requests. Failures before the
/// scan starts return an error; cancellation or provider failure during paging retains durable
/// partial scan evidence in the returned report.
pub async fn enumerate_repository_threads(
    archive: &Archive,
    client: &GitHubClient,
    selector: &RepositorySelector,
    cancellation: &CancellationToken,
) -> Result<ThreadEnumerationReport, EngineError> {
    let started_at = now_utc()?;
    let sequence = archive.reserve_observation_sequence(started_at).await?;
    let repository = fetch_repository(
        client,
        selector.host(),
        selector.owner(),
        selector.name(),
        cancellation,
    )
    .await?;
    archive.upsert_repository(&repository).await?;

    enumerate_repository_thread_pages(
        archive,
        client,
        ThreadScanContext {
            repository,
            sequence,
            started_at,
            state: ThreadListState::All,
            since: None,
        },
        None,
        cancellation,
    )
    .await
}
