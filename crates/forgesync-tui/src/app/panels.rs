//! Panel state: each panel's [`Loadable`] read plus the selection it owns.

use forgesync_core::content::Repository;
use forgesync_store::clusters::{ClusterDetail, ClusterSummary};
use forgesync_store::reads::{ThreadDetail, ThreadSummary};
use ratatui::widgets::ListState;

use crate::app::loadable::Loadable;
use crate::query::failures::RunFailureSummary;

/// Keeps a selection on an existing row, selecting the first row when nothing was selected.
fn clamp_selection(state: &mut ListState, len: usize) {
    let selected = state.selected().unwrap_or(0);
    state.select((len > 0).then(|| selected.min(len - 1)));
}

/// A loaded list and its highlighted row.
#[derive(Debug)]
pub struct ListPanel<T> {
    pub rows: Loadable<Vec<T>>,
    pub state: ListState,
}

impl<T> Default for ListPanel<T> {
    fn default() -> Self {
        Self {
            rows: Loadable::default(),
            state: ListState::default(),
        }
    }
}

impl<T> ListPanel<T> {
    /// The highlighted row, if any.
    pub fn selected(&self) -> Option<&T> {
        self.state
            .selected()
            .and_then(|index| self.rows.data.get(index))
    }

    /// Moves the highlight with a `ListState` step, keeping it on a loaded row.
    pub fn select(&mut self, step: impl FnOnce(&mut ListState)) {
        step(&mut self.state);
        clamp_selection(&mut self.state, self.rows.data.len());
    }

    /// Keeps the highlight on a row after new rows arrive, starting at the first row.
    pub fn loaded(&mut self) {
        clamp_selection(&mut self.state, self.rows.data.len());
    }
}

/// Repository rows, the highlighted row, and the applied browsing scope.
#[derive(Debug)]
pub struct RepositoryPicker {
    pub rows: Loadable<Vec<Repository>>,
    /// Highlighted row; row zero is the synthetic all-repositories row before `rows`.
    pub state: ListState,
    /// Scope for thread reads and writer actions; `None` selects every repository.
    pub applied: Option<Repository>,
}

impl Default for RepositoryPicker {
    fn default() -> Self {
        Self {
            rows: Loadable::default(),
            state: ListState::default().with_selected(Some(0)),
            applied: None,
        }
    }
}

impl RepositoryPicker {
    /// The highlighted repository; `None` for the all-repositories row.
    pub fn highlighted(&self) -> Option<&Repository> {
        let index = self.state.selected()?.checked_sub(1)?;
        self.rows.data.get(index)
    }

    /// Moves the highlight, counting the synthetic all-repositories row.
    pub fn select(&mut self, step: impl FnOnce(&mut ListState)) {
        step(&mut self.state);
        clamp_selection(&mut self.state, self.rows.data.len() + 1);
    }

    /// Clamps the highlight and refreshes the applied repository by provider identity.
    ///
    /// The applied repository stays applied when it is missing from the new rows, so a refresh
    /// can neither retarget a writer nor broaden its scope to every repository.
    pub fn loaded(&mut self) {
        clamp_selection(&mut self.state, self.rows.data.len() + 1);
        if let Some(applied) = &mut self.applied
            && let Some(current) = self.rows.data.iter().find(|row| row.id == applied.id)
        {
            applied.clone_from(current);
        }
    }
}

/// One page of discussions and the selected row.
#[derive(Debug, Default)]
pub struct ThreadList {
    pub rows: Loadable<Vec<ThreadSummary>>,
    pub state: ListState,
    /// Offset of the latest applied page, reused when reloading it.
    pub offset: u64,
    pub next_offset: Option<u64>,
}

impl ThreadList {
    /// Starts a read and clears the old rows: a new query or scope must not show stale results.
    pub fn begin(&mut self) -> u64 {
        self.rows.data.clear();
        self.state.select(None);
        self.next_offset = None;
        self.rows.begin()
    }

    /// Records the applied page coordinates and selects the first row.
    pub fn loaded(&mut self, offset: u64, next_offset: Option<u64>) {
        self.offset = offset;
        self.next_offset = next_offset;
        clamp_selection(&mut self.state, self.rows.data.len());
    }

    /// The selected discussion, if any.
    pub fn selected(&self) -> Option<&ThreadSummary> {
        self.state
            .selected()
            .and_then(|index| self.rows.data.get(index))
    }

    /// Moves the selection with a `ListState` step, keeping it on a loaded row.
    pub fn select(&mut self, step: impl FnOnce(&mut ListState)) {
        step(&mut self.state);
        clamp_selection(&mut self.state, self.rows.data.len());
    }
}

/// The selected discussion; cleared whenever the thread selection changes.
#[derive(Debug, Default)]
pub struct DetailPane {
    pub detail: Loadable<Option<Box<ThreadDetail>>>,
    /// Requested scroll; rendering clamps it to the content and viewport.
    pub scroll: u16,
}

impl DetailPane {
    /// Clears the detail and rejects its pending reply.
    pub fn invalidate(&mut self) {
        self.detail.reset();
        self.scroll = 0;
    }

    /// Starts a read without showing the previous discussion meanwhile.
    pub fn begin(&mut self) -> u64 {
        self.invalidate();
        self.detail.begin()
    }
}

pub type FailureList = ListPanel<RunFailureSummary>;
pub type ClusterList = ListPanel<ClusterSummary>;

/// The opened cluster's members and the highlighted member.
#[derive(Debug, Default)]
pub struct ClusterDetailPane {
    pub detail: Loadable<Option<ClusterDetail>>,
    pub members: ListState,
}

impl ClusterDetailPane {
    /// Starts a read, keeping cached members only when they belong to the requested cluster.
    ///
    /// Clearing another cluster's members immediately keeps member decisions from targeting the
    /// previous cluster while this one loads.
    pub fn begin(&mut self, cluster_id: u64) -> u64 {
        if self
            .detail
            .data
            .as_ref()
            .is_none_or(|detail| detail.cluster.id != cluster_id)
        {
            self.detail.data = None;
            self.members.select(None);
        }
        self.detail.begin()
    }

    fn member_count(&self) -> usize {
        self.detail
            .data
            .as_ref()
            .map_or(0, |detail| detail.members.len())
    }

    /// Keeps the member highlight on a loaded member.
    pub fn loaded(&mut self) {
        let count = self.member_count();
        clamp_selection(&mut self.members, count);
    }

    /// Moves the member highlight with a `ListState` step.
    pub fn select_member(&mut self, step: impl FnOnce(&mut ListState)) {
        step(&mut self.members);
        self.loaded();
    }
}

#[cfg(test)]
mod tests;
