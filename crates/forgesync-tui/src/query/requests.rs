//! # Express terminal read and maintainer intent
//!
//! [`QueryAction`] is the boundary between keyboard/navigation decisions and asynchronous work.
//! The app builds these values from its applied repository scope and loaded selections; query
//! dispatch performs the requested read or operation. Completion travels back through
//! [`crate::app::messages::QueryMessage`], rather than changing navigation from a background task.
//!
//! Read variants identify local archive projections. Sync, refresh, and retry may contact GitHub;
//! cluster decisions write local maintainer state without GitHub write-back. Separate dismiss,
//! restore, exclude, and include variants make the requested transition visible at construction.
//! An empty repository scope means all registered repositories, not the highlighted picker row.
//! Only cluster-detail reads carry a generation here: the app begins that pane before dispatch.
//! Other read starters obtain their generation from the relevant panel when they begin loading.

use forgesync_core::identity::RunId;
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};

/// One explicit read, writer action, or cancellation request from terminal interaction.
///
/// Values describe intent only: constructing one does not open an archive, start I/O, or reserve
/// the writer. Dispatch admits writer work through the app's operation display and registers it
/// with the task owner. Reads remain independently concurrent and reject stale panel replies.
#[derive(Debug, Eq, PartialEq)]
pub enum QueryAction {
    /// Reloads the registered repository picker from the archive.
    Repositories,
    /// Reads a bounded discussion page, optionally ranked by local keyword search.
    Threads {
        /// Submitted keyword query; `None` browses discussions by update time.
        query: Option<String>,
        /// Applied repository scope; empty selects all archived repositories.
        repositories: Vec<RepositorySelector>,
        /// Zero-based row offset within that scope and query.
        offset: u64,
    },
    /// Loads a discussion's archived content and evidence timeline.
    Detail(ThreadSelector),
    /// Reloads archive-wide coverage and health diagnostics.
    Coverage,
    /// Reloads recent unfinished run summaries and unresolved failures.
    Failures,
    /// Reads duplicate-cluster summaries for the applied repository scope.
    Clusters {
        /// Selected repositories; empty selects all registered repositories.
        repositories: Vec<RepositorySelector>,
    },
    /// Loads members of a cluster whose pane has already begun loading.
    ClusterDetail {
        /// Generation reserved by the cluster detail owner before dispatch.
        generation: u64,
        /// Archive-local cluster identity selected by the user.
        id: u64,
    },
    /// Acquires discussion metadata for the selected repositories from GitHub.
    Sync {
        /// Applied scope; empty resolves to all registered repositories before acquisition.
        repositories: Vec<RepositorySelector>,
    },
    /// Refreshes discussions, comments, reviews, and review threads from GitHub.
    Refresh {
        /// Applied scope; empty resolves to all registered repositories before acquisition.
        repositories: Vec<RepositorySelector>,
    },
    /// Retries unresolved work recorded under this archive-local run identity.
    Retry(RunId),
    /// Records a local dismissal without removing the cluster's evidence.
    DismissCluster {
        /// Archive-local cluster identity to dismiss.
        id: u64,
    },
    /// Removes the local dismissal from a cluster.
    RestoreCluster {
        /// Archive-local cluster identity to restore.
        id: u64,
    },
    /// Excludes one member from local cluster triage.
    ExcludeClusterMember {
        /// Archive-local cluster whose membership is being changed.
        id: u64,
        /// Loaded member identity; resolved by the engine against this archive.
        reference: ThreadSelector,
    },
    /// Restores an excluded member to local cluster triage.
    IncludeClusterMember {
        /// Archive-local cluster whose membership is being changed.
        id: u64,
        /// Loaded member identity; resolved by the engine against this archive.
        reference: ThreadSelector,
    },
    /// Chooses the member used to represent the local cluster.
    SetCanonicalClusterMember {
        /// Archive-local cluster whose representative is being changed.
        id: u64,
        /// Loaded member identity; resolved by the engine against this archive.
        reference: ThreadSelector,
    },
    /// Signals cooperative cancellation of the tracked writer, if one exists.
    CancelOperation,
}
