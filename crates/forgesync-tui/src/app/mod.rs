//! # State machine for interactive browsing
//!
//! `App` is the single source of current screen, focus, selection, loaded data, and pending query
//! messages. `Screen` and `Focus` make navigation modes explicit; `QueryMessage` carries
//! asynchronous results back to the state machine.
//!
//! `input` maps keys to actions, and `state` applies data or transitions. Drawing code reads the
//! current app state through `view` but should not initiate archive operations. This split keeps a
//! user action traceable from key event to query request, result, and rendered screen.

use forgesync_core::content::Repository;
use forgesync_engine::sync::SyncProgress;
use forgesync_store::clusters::{ClusterDetail, ClusterPage, ClusterSummary};
use forgesync_store::reads::{ArchiveStatus, ThreadDetail, ThreadPage, ThreadSummary};
use forgesync_store::runs::RunStatus;

use crate::query::QueryAction;

const PAGE_SIZE: u32 = 100;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Screen {
    #[default]
    Browser,
    Coverage,
    Failures,
    Clusters,
    ClusterDetail,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Focus {
    #[default]
    Repositories,
    Threads,
    Detail,
}

#[derive(Debug, Default)]
pub struct App {
    pub screen: Screen,
    pub focus: Focus,
    pub repository_cursor: usize,
    pub repositories: Vec<Repository>,
    pub applied_repository: Option<usize>,
    pub repository_generation: u64,
    pub repositories_loading: bool,
    pub repository_error: Option<String>,
    pub threads: Vec<ThreadSummary>,
    pub selected_thread: Option<usize>,
    pub page_offset: u64,
    pub next_offset: Option<u64>,
    pub thread_generation: u64,
    pub threads_loading: bool,
    pub thread_error: Option<String>,
    pub detail: Option<ThreadDetail>,
    pub detail_generation: u64,
    pub detail_loading: bool,
    pub detail_error: Option<String>,
    pub detail_scroll: u16,
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
    pub operation_generation: u64,
    pub operation_busy: bool,
    pub operation_label: Option<String>,
    pub operation_progress: Option<SyncProgress>,
    pub search_query: Option<String>,
    pub search_input: String,
    pub searching: bool,
    pub status: Option<String>,
    pub quit: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunFailureSummary {
    pub id: u64,
    pub status: RunStatus,
    pub entries: Vec<String>,
}

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
    OperationProgress {
        generation: u64,
        progress: SyncProgress,
    },
    OperationFinished {
        generation: u64,
        result: Result<String, String>,
    },
}

mod input;
mod state;

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
