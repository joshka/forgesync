use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use forgesync_core::content::Repository;
use forgesync_core::identity::RunId;
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};
use forgesync_engine::sync::SyncProgress;
use forgesync_store::clusters::{ClusterDetail, ClusterPage, ClusterSummary};
use forgesync_store::reads::{ArchiveStatus, ThreadDetail, ThreadPage, ThreadSummary};
use forgesync_store::runs::RunStatus;

use crate::query::QueryAction;

const PAGE_SIZE: u32 = 100;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Screen {
    #[default]
    Browser,
    Coverage,
    Failures,
    Clusters,
    ClusterDetail,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Focus {
    #[default]
    Repositories,
    Threads,
    Detail,
}

#[derive(Debug, Default)]
pub(crate) struct App {
    pub(crate) screen: Screen,
    pub(crate) focus: Focus,
    pub(crate) repository_cursor: usize,
    pub(crate) repositories: Vec<Repository>,
    pub(crate) applied_repository: Option<usize>,
    pub(crate) repository_generation: u64,
    pub(crate) repositories_loading: bool,
    pub(crate) repository_error: Option<String>,
    pub(crate) threads: Vec<ThreadSummary>,
    pub(crate) selected_thread: Option<usize>,
    pub(crate) page_offset: u64,
    pub(crate) next_offset: Option<u64>,
    pub(crate) thread_generation: u64,
    pub(crate) threads_loading: bool,
    pub(crate) thread_error: Option<String>,
    pub(crate) detail: Option<ThreadDetail>,
    pub(crate) detail_generation: u64,
    pub(crate) detail_loading: bool,
    pub(crate) detail_error: Option<String>,
    pub(crate) detail_scroll: u16,
    pub(crate) coverage: Option<ArchiveStatus>,
    pub(crate) coverage_generation: u64,
    pub(crate) coverage_loading: bool,
    pub(crate) coverage_error: Option<String>,
    pub(crate) failures: Vec<RunFailureSummary>,
    pub(crate) failures_generation: u64,
    pub(crate) failures_loading: bool,
    pub(crate) failures_error: Option<String>,
    pub(crate) selected_failure: usize,
    pub(crate) clusters: Vec<ClusterSummary>,
    pub(crate) selected_cluster: usize,
    pub(crate) clusters_generation: u64,
    pub(crate) clusters_loading: bool,
    pub(crate) clusters_error: Option<String>,
    pub(crate) cluster_detail: Option<ClusterDetail>,
    pub(crate) cluster_detail_generation: u64,
    pub(crate) cluster_detail_loading: bool,
    pub(crate) cluster_detail_error: Option<String>,
    pub(crate) selected_cluster_member: usize,
    pub(crate) operation_generation: u64,
    pub(crate) operation_busy: bool,
    pub(crate) operation_label: Option<String>,
    pub(crate) operation_progress: Option<SyncProgress>,
    pub(crate) search_query: Option<String>,
    pub(crate) search_input: String,
    pub(crate) searching: bool,
    pub(crate) status: Option<String>,
    pub(crate) quit: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RunFailureSummary {
    pub(crate) id: u64,
    pub(crate) status: RunStatus,
    pub(crate) entries: Vec<String>,
}

pub(crate) enum QueryMessage {
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

fn move_index(current: usize, max: usize, direction: i8) -> usize {
    if direction < 0 {
        current.saturating_sub(1)
    } else {
        current.saturating_add(1).min(max)
    }
}

#[cfg(test)]
mod tests;
