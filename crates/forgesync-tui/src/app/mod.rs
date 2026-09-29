//! # Coordinate interactive browsing and background results
//!
//! [`App`] keeps the active screen, browser focus, loaded projections, search draft, and status
//! line. [`Screen`] selects the workflow being shown; [`Focus`] chooses the browser pane whose
//! selection responds to navigation. Switching an inspection screen does not apply a new filter.
//!
//! [`repositories::RepositoryPicker`] owns repository choices, the applied scope, and its read
//! lifecycle. [`operation::OperationDisplay`] owns idle/running writer presentation. The other
//! projections remain coordinated here so a scope or thread selection change invalidates the
//! corresponding detail rather than leaving an unrelated result visible.
//!
//! `input` maps keys to named actions. `state` starts generations and applies [`QueryMessage`]
//! results only when they still belong to the requested panel. Query tasks perform archive work
//! asynchronously; the drawing code reads state without initiating archive operations. This path
//! keeps a key event traceable through request, result, and rendered screen.
//!
//! Construct the app with [`Default::default`], request its initial repository/thread reads, and
//! feed current-generation replies through `apply`. The app does not own a terminal, archive pool,
//! or runtime. [`RunFailureSummary`] is a safe presentation projection for selecting a run retry;
//! the engine and store retain the complete ledger and decide what work is retryable.

use forgesync_core::content::Repository;
use forgesync_engine::sync::SyncProgress;
use forgesync_store::clusters::{ClusterDetail, ClusterPage, ClusterSummary};
use forgesync_store::reads::{ArchiveStatus, ThreadDetail};
use forgesync_store::runs::RunStatus;

/// Bounded thread-page size used by browser paging; independent of the CLI default.
const PAGE_SIZE: u32 = 100;

/// Active browsing or maintainer workflow; each screen interprets navigation in its own scope.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Screen {
    #[default]
    /// Repository picker, discussion list, and selected discussion detail.
    Browser,
    /// Archive-wide evidence completeness and diagnostic counts.
    Coverage,
    /// Recent failed or partial runs that can be selected for retry.
    Failures,
    /// Duplicate clusters for the currently applied repository scope.
    Clusters,
    /// One cluster and its members, with local triage decisions.
    ClusterDetail,
}

/// Browser pane that receives movement and selection keys; retained across inspection screens.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Focus {
    #[default]
    /// Highlight repository choices without applying a filter until selection.
    Repositories,
    /// Move discussion selection and invalidate any previous detail.
    Threads,
    /// Scroll the selected discussion without changing the list selection.
    Detail,
}

/// Coordination state consumed by input, asynchronous query dispatch, and rendering.
///
/// Default state shows the browser with repository focus and no loaded projections. Reads carry
/// independent generations so a late result cannot overwrite a newer scope or selection. Writer
/// execution lives in query tasks; only its presentation and keyboard cancellation intent live
/// here.
#[derive(Debug, Default)]
pub struct App {
    pub screen: Screen,
    pub focus: Focus,
    /// Repository choices, applied scope, and pending repository read state.
    pub repository_picker: repositories::RepositoryPicker,
    /// Discussion page, selection, pagination, and pending read state.
    pub thread_list: threads::ThreadList,
    /// Selected discussion projection, read lifecycle, and requested scroll position.
    pub detail_pane: detail::DetailPane,
    pub coverage: Option<ArchiveStatus>,
    pub coverage_generation: u64,
    pub coverage_loading: bool,
    pub coverage_error: Option<String>,
    pub failures: Vec<RunFailureSummary>,
    pub failures_generation: u64,
    pub failures_loading: bool,
    pub failures_error: Option<String>,
    pub selected_failure: usize,
    pub clusters: Vec<ClusterSummary>,
    pub selected_cluster: usize,
    pub clusters_generation: u64,
    pub clusters_loading: bool,
    pub clusters_error: Option<String>,
    pub cluster_detail: Option<ClusterDetail>,
    pub cluster_detail_generation: u64,
    pub cluster_detail_loading: bool,
    pub cluster_detail_error: Option<String>,
    pub selected_cluster_member: usize,
    /// Current writer generation and its transient label/progress display.
    pub operation: operation::OperationDisplay,
    pub search_query: Option<String>,
    pub search_input: String,
    pub searching: bool,
    pub status: Option<String>,
    pub quit: bool,
}

/// Safe run-list projection that keeps retry selection separate from the complete failure ledger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunFailureSummary {
    /// Archive-local run identity used when requesting a selected retry.
    pub id: u64,
    /// Durable run state displayed beside its failure summaries.
    pub status: RunStatus,
    /// Safe human-readable job and failure summaries; never raw provider payloads.
    pub entries: Vec<String>,
}

pub enum QueryMessage {
    Repositories {
        generation: u64,
        result: Result<Vec<Repository>, String>,
    },
    Threads(threads::ThreadReply),
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
    OperationProgress {
        generation: u64,
        progress: SyncProgress,
    },
    OperationFinished {
        generation: u64,
        result: Result<String, String>,
    },
}

pub mod detail;
mod input;
pub mod operation;
pub mod repositories;
mod state;
pub mod threads;

/// Moves a bounded selection by one row without underflow or overshoot.
fn move_index(current: usize, max: usize, direction: i8) -> usize {
    if direction < 0 {
        current.saturating_sub(1)
    } else {
        current.saturating_add(1).min(max)
    }
}

#[cfg(test)]
mod tests;
