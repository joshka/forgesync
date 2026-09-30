//! # Offline thread projections
//!
//! `ThreadQuery` selects scope, state, sort, and pagination; `ThreadPage` and `ThreadSummary`
//! return list results. `ThreadDetail` and timeline types assemble the discussion and its
//! observations for show operations. Coverage summaries explain which resource families are
//! complete or partial.
//!
//! `query` owns filtered list SQL, `detail` assembles a thread, and `coverage` reads evidence
//! state; `summary` aggregates family coverage and archive totals. These are projections over the
//! archive, not provider fetches. Engine inspection and search can use them without opening a
//! network client or understanding SQL row layouts.

use std::num::NonZeroU32;

use forgesync_core::content::{
    Comment, Discussion, PullRequestMetadata, Repository, Review, ReviewThread, ThreadKind,
};
use forgesync_core::coverage::{Coverage, CoverageState, EvidenceFamily};
use forgesync_core::identity::{
    CommitSha, RepositoryId, ReviewThreadId, ThreadId, ThreadReference,
};
use forgesync_core::timestamp::UtcTimestamp;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::archive::{Archive, ArchiveInfo};
use crate::diagnostics::ArchiveDiagnostics;
use crate::error::StoreError;
use crate::observations::StagedItem;

/// Source-state filter for a local discussion query.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThreadStateFilter {
    /// Include open, closed, and unrecognized source states.
    #[default]
    All,
    /// Include only discussions whose source state is open.
    Open,
    /// Include only discussions whose source state is closed.
    Closed,
}

/// Sort order for local discussion queries.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThreadSort {
    /// Rank full-text matches first; without a query, use update order.
    #[default]
    Relevance,
    /// Sort by source update time, newest first.
    Updated,
    /// Sort by source creation time, newest first.
    Created,
}

/// Typed filters and pagination for a local thread search or list.
#[derive(Clone, Debug)]
pub struct ThreadQuery {
    /// Resolved repository identities; an empty list includes every repository.
    pub repositories: Vec<RepositoryId>,
    /// Optional issue or pull-request filter.
    pub kind: Option<ThreadKind>,
    /// Source open/closed filter.
    pub state: ThreadStateFilter,
    /// Prepared FTS5 expression. `None` lists without text search.
    pub match_expression: Option<String>,
    /// Optional inclusive lower bound on the current source update timestamp.
    pub updated_since: Option<UtcTimestamp>,
    /// Sort policy, applied with deterministic ties.
    pub sort: ThreadSort,
    /// Maximum number of rows to return.
    pub limit: NonZeroU32,
    /// Number of matching rows to skip.
    pub offset: u64,
}

/// One discussion and its containing repository with current family coverage.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ThreadSummary {
    /// Current repository identity and display metadata.
    pub repository: Repository,
    /// Current normalized source discussion.
    pub discussion: Discussion,
    /// Current coverage for applicable evidence families.
    pub coverage: Vec<Coverage>,
}

/// A page of local discussion results and archive coverage for the repository scope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ThreadPage {
    /// Results in stable query order.
    pub items: Vec<ThreadSummary>,
    /// Offset to pass to the next request, when more results are available.
    pub next_offset: Option<u64>,
    /// Coverage totals for the selected repositories, even when no rows match.
    pub coverage: Vec<FamilyCoverageSummary>,
}

/// Current discussion content and selected family evidence for a thread reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ThreadDetail {
    /// Discussion, repository, and per-family coverage.
    pub summary: ThreadSummary,
    /// Current comments, when comment coverage has been acquired.
    pub comments: Vec<StagedItem<Comment>>,
    /// Current pull-request metadata, when acquired.
    pub pull_request_metadata: Vec<StagedItem<PullRequestMetadata>>,
    /// Current submitted reviews, when acquired.
    pub reviews: Vec<StagedItem<Review>>,
    /// Current review threads, when acquired.
    pub review_threads: Vec<StagedItem<ReviewThread>>,
    /// Chronological projection of current source content, not full revision history.
    pub timeline: Vec<ThreadTimelineEntry>,
}

/// One current source event ordered by its source timestamp when available.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ThreadTimelineEntry {
    /// Source timestamp; missing review timestamps sort after known events.
    pub occurred_at: Option<UtcTimestamp>,
    /// Typed event with the selected content and review context.
    pub event: ThreadTimelineEvent,
}

/// Event represented in the current thread timeline.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ThreadTimelineEvent {
    /// Original source creation event for the current discussion.
    ThreadCreated {
        /// Stable thread identity.
        thread: ThreadId,
        /// Current title.
        title: String,
    },
    /// Source closure time retained on the current discussion.
    ThreadClosed {
        /// Stable thread identity.
        thread: ThreadId,
    },
    /// Current issue or discussion comment.
    Comment {
        /// Normalized comment content and identity.
        comment: Comment,
    },
    /// Current submitted review.
    Review {
        /// Normalized review content and identity.
        review: Review,
    },
    /// A current review-thread state, which has no reliable independent event time.
    ReviewThread {
        /// Stable review-thread identity.
        id: ReviewThreadId,
        /// Pull-request head against which resolution was read.
        head_sha: CommitSha,
        /// Whether the source currently reports the thread resolved.
        is_resolved: bool,
        /// Whether the source currently reports the line outdated.
        is_outdated: bool,
        /// Current reviewed path, when available.
        path: Option<String>,
    },
    /// A current comment nested in a review thread.
    ReviewThreadComment {
        /// Stable parent review-thread identity.
        review_thread_id: ReviewThreadId,
        /// Pull-request head against which the parent state was read.
        head_sha: CommitSha,
        /// Current resolution state of the parent review thread.
        is_resolved: bool,
        /// Current outdated state of the parent review thread.
        is_outdated: bool,
        /// Current reviewed path, when available.
        path: Option<String>,
        /// Normalized nested comment.
        comment: Comment,
    },
}

/// Complete, incomplete, and missing counts for one applicable evidence family.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FamilyCoverageSummary {
    /// Family whose current coverage is counted.
    pub family: EvidenceFamily,
    /// Number of source threads to which this family applies.
    pub applicable_threads: u64,
    /// Applicable threads without an observation.
    pub missing: u64,
    /// Applicable threads with an incomplete latest collection.
    pub incomplete: u64,
    /// Applicable threads with a complete latest collection.
    pub complete: u64,
}

/// Archive metadata, local content counts, and family coverage totals.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ArchiveStatus {
    /// Validated lifecycle metadata.
    pub archive: ArchiveInfo,
    /// Number of registered repositories.
    pub repositories: u64,
    /// Number of archived issue and pull-request discussions.
    pub threads: u64,
    /// Number of archived issues.
    pub issues: u64,
    /// Number of archived pull requests.
    pub pull_requests: u64,
    /// Coverage totals across all applicable threads.
    pub coverage: Vec<FamilyCoverageSummary>,
    /// Schema, lease, and durable-work diagnostics.
    pub diagnostics: ArchiveDiagnostics,
}

struct StoredThreadSummary {
    row_id: i64,
    summary: ThreadSummary,
}

pub(crate) struct StoredCoverage {
    state: CoverageState,
    source_clock_state: String,
    source_clock_us: Option<i64>,
    snapshot_head_sha: Option<String>,
    current_head_sha: Option<String>,
}

const ALL_FAMILIES: [EvidenceFamily; 5] = [
    EvidenceFamily::Threads,
    EvidenceFamily::Comments,
    EvidenceFamily::PullRequestMetadata,
    EvidenceFamily::Reviews,
    EvidenceFamily::ReviewThreads,
];

mod coverage;
mod detail;
mod query;
mod summary;

pub(crate) use coverage::{coverage_for_kind, load_thread_coverage};
pub(crate) use query::{push_discussion_filters, push_repository_scope};
