//! # Coordinate interactive browsing and background results
//!
//! [`App`] keeps the active screen, browser focus, loaded projections, search draft, and status
//! line. [`Screen`] selects the workflow being shown; [`Focus`] chooses the browser pane whose
//! selection responds to navigation. Switching an inspection screen does not apply a new filter.
//!
//! [`repositories::RepositoryPicker`] owns choices and applied scope. [`threads::ThreadList`] owns
//! discussion pages, while [`detail::DetailPane`] owns the selected discussion's evidence and
//! scroll. [`coverage::CoveragePanel`] and [`failures::FailureList`] show archive completeness and
//! retry choices. [`clusters::ClusterList`] and [`clusters::ClusterDetailPane`] show scoped
//! duplicate groups and their current members. [`operation::OperationDisplay`] owns idle/running
//! writer presentation. Each panel carries its own generation and cache policy; the app coordinates
//! changes that affect another panel, such as invalidating detail when discussion selection or
//! repository scope changes.
//!
//! `input` maps keys to named actions. `state` starts generations and applies
//! [`messages::QueryMessage`] results only when they still belong to the requested panel. Query
//! tasks perform archive work asynchronously; the drawing code reads state without initiating
//! archive operations. This path keeps a key event traceable through request, result, and rendered
//! screen.
//!
//! Construct the app with [`Default::default`], request its initial repository/thread reads, and
//! feed current-generation replies through `apply`. The app does not own a terminal, archive pool,
//! or runtime. [`failures::RunFailureSummary`] is a safe presentation projection for selecting a
//! run retry; the engine and store retain the complete ledger and decide what work is retryable.

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
    /// Active workflow; screen changes do not implicitly change the applied repository filter.
    pub screen: Screen,
    /// Browser pane receiving navigation, retained while inspecting another screen.
    pub focus: Focus,
    /// Repository choices, applied scope, and pending repository read state.
    pub repository_picker: repositories::RepositoryPicker,
    /// Discussion page, selection, pagination, and pending read state.
    pub thread_list: threads::ThreadList,
    /// Selected discussion projection, read lifecycle, and requested scroll position.
    pub detail_pane: detail::DetailPane,
    /// Archive-wide coverage projection and its independent refresh state.
    pub coverage_panel: coverage::CoveragePanel,
    /// Failed-run choices, retry selection, and pending ledger refresh.
    pub failure_list: failures::FailureList,
    /// Scoped cluster choices, highlight, and pending refresh.
    pub cluster_list: clusters::ClusterList,
    /// Selected cluster membership and its independent detail refresh.
    pub cluster_detail_pane: clusters::ClusterDetailPane,
    /// Current writer generation and its transient label/progress display.
    pub operation: operation::OperationDisplay,
    /// Applied local keyword query; absent when browsing without text filtering.
    pub search_query: Option<String>,
    /// Unsubmitted search draft; editing it does not change current discussion results.
    pub search_input: String,
    /// Whether keyboard input edits the draft instead of navigating the current screen.
    pub searching: bool,
    /// Safe action summary, read failure, or cancellation notice shown in the footer.
    pub status: Option<String>,
    /// Terminal exit intent, set only after no writer requires cancellation cleanup.
    pub quit: bool,
}

pub mod clusters;
pub mod coverage;
pub mod detail;
pub mod failures;
mod input;
pub mod messages;
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

#[cfg(test)]
mod test_data;
