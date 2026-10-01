//! Results sent from background tasks to the app.
//!
//! Read replies carry the generation their panel began; errors are safe display strings. Large
//! projections are boxed to keep the enum small.

use forgesync_core::content::Repository;
use forgesync_engine::sync::SyncProgress;
use forgesync_store::clusters::{ClusterDetail, ClusterPage};
use forgesync_store::reads::{ArchiveStatus, ThreadDetail, ThreadPage};

use crate::query::failures::RunFailureSummary;

/// A background result for the app.
pub enum QueryMessage {
    Repositories {
        generation: u64,
        result: Result<Vec<Repository>, String>,
    },
    Threads {
        generation: u64,
        offset: u64,
        result: Result<Box<ThreadPage>, String>,
    },
    Detail {
        generation: u64,
        result: Result<Box<ThreadDetail>, String>,
    },
    Coverage {
        generation: u64,
        result: Result<Box<ArchiveStatus>, String>,
    },
    Failures {
        generation: u64,
        result: Result<Vec<RunFailureSummary>, String>,
    },
    Clusters {
        generation: u64,
        result: Result<Box<ClusterPage>, String>,
    },
    ClusterDetail {
        generation: u64,
        result: Result<Box<ClusterDetail>, String>,
    },
    /// Advisory counters from the running writer; never implies success.
    ///
    /// Writer messages need no generation: a second writer cannot start until the first one's
    /// `OperationFinished` is applied, and its progress is drained into this ordered channel
    /// before that message is sent.
    OperationProgress(SyncProgress),
    OperationFinished(Result<String, String>),
}
