//! # Shared acquisition identities and accounting results
//!
//! This private module defines the values passed between sync coordination and family collectors.
//! `ScopeUnit` identifies one parent enumeration and its checkpoint authority. `SyncRunContext`
//! supplies immutable run, writer, cancellation, and progress capabilities to every job.
//!
//! `ThreadFamilyScope` ties repository, thread, source time, and ledger key together before a child
//! acquisition. `PullRequestTarget` is the owned local selection from which that borrowed scope is
//! built. `ThreadFamilyResult` keeps received/committed counts, payload, failure, and interruption
//! distinct until a job folds them into its durable outcome.
//!
//! These values do not open transactions or perform provider requests. Family collectors own
//! reserved observations and pagination; job owners retain mutable accounting. `units` expands the
//! requested thread scope into independent open/closed/all enumerations without allocating order.

use forgesync_core::coverage::Failure;
use forgesync_core::identity::{RunId, ThreadId};
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_github::resources::ThreadListState;
use forgesync_store::leases::ArchiveLeaseToken;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::sync::{SyncProgress, SyncThreadScope};

/// One durable thread-state enumeration within the caller's selected sync scope.
///
/// Default sync expands into separate open and closed units so each has an independent ledger and
/// checkpoint. Only a complete scan authorized by this unit may advance the closed watermark.
#[derive(Clone, Copy)]
pub struct ScopeUnit {
    /// Stable ledger scope key, independent of a provider page cursor.
    pub key: &'static str,
    /// Provider filter for this enumeration.
    pub state: ThreadListState,
    /// Whether complete enumeration establishes a new closed-sweep checkpoint.
    pub update_closed_watermark: bool,
}

/// Immutable run identity and execution policy shared by repository-family job owners.
///
/// The coordinator owns the lease and child cancellation token. Jobs borrow those capabilities;
/// they cannot silently select a different run or start an unrelated writer. Mutable acquisition
/// progress stays in each job and in `WorkSummary`, rather than in this shared execution scope.
pub struct SyncRunContext<'a> {
    /// Initial repository/scope job count; `WorkSummary` adds pull-request family jobs for
    /// nonempty scopes.
    pub total_jobs: u64,
    /// Whether this run acquires the independent discussion-comment family.
    pub include_comments: bool,
    /// Whether this run acquires pull-request reviews.
    pub include_reviews: bool,
    /// Whether this run acquires review threads and their nested comments.
    pub include_review_threads: bool,
    /// Persisted run receiving job and failure records.
    pub run_id: RunId,
    /// Writer fence checked by every durable mutation.
    pub lease: &'a ArchiveLeaseToken,
    /// Operation token cancelled by the caller or a failed lease renewal.
    pub cancellation: &'a CancellationToken,
    /// Optional bounded snapshot channel; a slow receiver never blocks writes.
    pub progress: Option<mpsc::Sender<SyncProgress>>,
}

/// Expands a requested thread scope into its independent provider enumerations.
pub fn units(scope: SyncThreadScope) -> Vec<ScopeUnit> {
    match scope {
        SyncThreadScope::Default => vec![ScopeUnit::OPEN, ScopeUnit::CLOSED],
        SyncThreadScope::Open => vec![ScopeUnit::OPEN],
        SyncThreadScope::Closed => vec![ScopeUnit::CLOSED],
        SyncThreadScope::All => vec![ScopeUnit::ALL],
    }
}

impl ScopeUnit {
    /// Open enumeration never proves completion of the independent closed-thread sweep.
    const OPEN: Self = Self {
        key: "open",
        state: ThreadListState::Open,
        update_closed_watermark: false,
    };

    /// Complete closed enumeration advances the sweep checkpoint after replay overlap.
    const CLOSED: Self = Self {
        key: "closed",
        state: ThreadListState::Closed,
        update_closed_watermark: true,
    };

    /// Complete all-state enumeration also establishes complete closed-thread coverage.
    const ALL: Self = Self {
        key: "all",
        state: ThreadListState::All,
        update_closed_watermark: true,
    };
}

/// Acquisition evidence for one thread's metadata or independently paginated child family.
///
/// Received and committed counts differ when stale or incomplete evidence cannot replace current
/// membership. An absent payload is distinct from a complete empty collection. Failure and
/// interruption retain partial accounting for the job ledger and aggregate report.
pub struct ThreadFamilyResult<T> {
    /// Recorded pages credited to this result; interrupted review acquisition leaves this zero.
    pub pages_completed: u64,
    /// Provider records acquired, including records not accepted as current membership.
    pub items_received: u64,
    /// Records accepted by the store for this observation.
    pub items_committed: u64,
    /// Acquired payload when available; an empty payload can still be complete evidence.
    pub value: Option<T>,
    /// Safe provider or archive failure retained with partial counts.
    pub failure: Option<Failure>,
    /// Whether explicit cancellation interrupted this family acquisition.
    pub interrupted: bool,
}

impl<T> Default for ThreadFamilyResult<T> {
    /// Starts family accounting with zero counts and no acquired payload, failure, or interruption.
    /// A completed empty payload is recorded explicitly when acquisition finishes.
    fn default() -> Self {
        Self {
            pages_completed: 0,
            items_received: 0,
            items_committed: 0,
            value: None,
            failure: None,
            interrupted: false,
        }
    }
}

/// Archived pull request selected for metadata, reviews, and review-thread acquisition.
pub struct PullRequestTarget {
    /// Parent identity shared by all independently acquired pull-request families.
    pub thread: ThreadId,
    /// Parent source timestamp used to attribute family observations.
    pub updated_at: UtcTimestamp,
}

/// Borrowed parent identity and durable scope key for one thread's family job.
///
/// Metadata and review collectors share this attribution while retaining separate completeness and
/// acquisition sequences. The key identifies the ledger scope, not provider pagination state.
#[derive(Clone, Copy)]
pub struct ThreadFamilyScope<'a> {
    /// Normalized repository containing the parent discussion.
    pub repository: &'a forgesync_core::content::Repository,
    /// Parent pull-request identity for the acquired evidence.
    pub thread: &'a ThreadId,
    /// Parent source timestamp associated with this selected work.
    pub updated_at: UtcTimestamp,
    /// Stable job scope used for durable failure and retry attribution.
    pub key: &'a str,
}
