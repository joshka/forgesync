//! # Typed results returned to interactive state
//!
//! [`QueryMessage`] is the response boundary between background query tasks and the app. Each read
//! variant identifies its target panel and carries the generation reserved before that task began.
//! Panels reject older generations before changing their visible data, loading state, or errors.
//!
//! Thread pages use [`ThreadReply`] to keep generation and offset attached to their result.
//! Large projections remain boxed so the message enum does not hold a detailed discussion's
//! entire fixed layout inline. Owned collections still use memory proportional to archived
//! evidence. Result errors are safe presentation strings prepared at the
//! query boundary; provider payloads and credentials do not belong in these messages.
//!
//! Writer progress and completion use a separate operation generation. Progress is advisory and
//! never implies successful acquisition. Completion supplies a final status, while query task
//! coordination separately schedules archive reads after the writer terminates.
//! Use this enum for completed work and progress; [`crate::query::requests::QueryAction`] describes
//! intent.

use forgesync_core::content::Repository;
use forgesync_engine::sync::SyncProgress;
use forgesync_store::clusters::{ClusterDetail, ClusterPage};
use forgesync_store::reads::{ArchiveStatus, ThreadDetail};

use super::failures::RunFailureSummary;
use super::threads::ThreadReply;

/// One asynchronous read result or writer update delivered to its owning app panel.
pub enum QueryMessage {
    /// Repository choices for the picker; failure retains previously loaded rows.
    Repositories {
        /// Picker generation reserved when this read began.
        generation: u64,
        /// Current archive repositories or a safe read failure.
        result: Result<Vec<Repository>, String>,
    },
    /// Discussion page with the request's offset and thread-list generation attached.
    Threads(ThreadReply),
    /// Selected discussion and its evidence, replacing only the current detail selection.
    Detail {
        /// Detail generation reserved before reading the selected reference.
        generation: u64,
        /// Canonical detail or a safe read failure; failure leaves no visible detail content.
        result: Result<Box<ThreadDetail>, String>,
    },
    /// Archive-wide diagnostics and evidence counts for the coverage screen.
    Coverage {
        /// Coverage read generation, independent of browser page requests.
        generation: u64,
        /// Current status projection or a safe failure; prior coverage can remain cached.
        result: Result<Box<ArchiveStatus>, String>,
    },
    /// Recent failed or partial run summaries used for selecting a retry.
    Failures {
        /// Failed-run list generation reserved before scanning the ledger.
        generation: u64,
        /// Safe run summaries or a read failure; the complete ledger remains in the archive.
        result: Result<Vec<RunFailureSummary>, String>,
    },
    /// Duplicate clusters for the applied repository scope.
    Clusters {
        /// Cluster-list generation reserved for this scoped read.
        generation: u64,
        /// Ordered cluster page or a safe read failure.
        result: Result<Box<ClusterPage>, String>,
    },
    /// One cluster and its current members for inspection and local decisions.
    ClusterDetail {
        /// Detail generation belonging to the requested cluster, independent of the list read.
        generation: u64,
        /// Cluster/member projection or a safe read failure.
        result: Result<Box<ClusterDetail>, String>,
    },
    /// Advisory counters from a running writer, accepted only for its active generation.
    OperationProgress {
        /// Writer generation reserved before acquisition or local analysis began.
        generation: u64,
        /// Bounded workflow counters without discussion bodies or credentials.
        progress: SyncProgress,
    },
    /// Terminal writer status; clears transient operation presentation when current.
    OperationFinished {
        /// Writer generation whose operation ended.
        generation: u64,
        /// Final human summary or safe error; durable partial outcomes remain in the run ledger.
        result: Result<String, String>,
    },
}
