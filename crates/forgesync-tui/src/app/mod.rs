//! Interactive state: screens, focus, panels, and the search draft.
//!
//! Input methods (`input`) return [`QueryAction`](crate::query::QueryAction)s for the event loop
//! to dispatch; `state` applies their results. Drawing only reads this state.

use forgesync_engine::sync::SyncProgress;
use forgesync_store::reads::ArchiveStatus;

/// Discussion page size used for previous-page offsets.
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

/// Browser pane receiving navigation keys; kept while another screen is shown.
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
    pub repository_picker: panels::RepositoryPicker,
    pub thread_list: panels::ThreadList,
    pub detail_pane: panels::DetailPane,
    pub coverage: loadable::Loadable<Option<ArchiveStatus>>,
    pub failure_list: panels::FailureList,
    pub cluster_list: panels::ClusterList,
    pub cluster_detail_pane: panels::ClusterDetailPane,
    /// The single running writer; shown in the footer in place of `status`.
    pub operation: Option<RunningOperation>,
    /// Submitted keyword query applied to thread reads.
    pub search_query: Option<String>,
    /// Unsubmitted search text; editing it does not change results.
    pub search_input: String,
    pub searching: bool,
    /// Footer status: an action result or read failure.
    pub status: Option<String>,
    /// Set only once no writer needs cancellation cleanup.
    pub quit: bool,
}

#[derive(Debug)]
pub struct RunningOperation {
    pub label: &'static str,
    pub progress: Option<SyncProgress>,
    /// Set once quitting has requested cancellation.
    pub cancelling: bool,
}

mod input;
pub mod loadable;
pub mod messages;
pub mod panels;
mod state;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub mod test_data;
