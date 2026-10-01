//! Panel state: each panel's [`Loadable`] read plus the selection it owns.

use forgesync_core::content::Repository;
use forgesync_store::clusters::{ClusterDetail, ClusterSummary};
use forgesync_store::reads::{ThreadDetail, ThreadSummary};

use crate::app::loadable::Loadable;
use crate::query::failures::RunFailureSummary;

#[derive(Debug, Default)]
pub struct RepositoryPicker {
    pub rows: Loadable<Vec<Repository>>,
    /// Highlighted row; zero is the synthetic all-repositories row before `rows`.
    pub cursor: usize,
    /// Scope for thread reads and writer actions; `None` selects every repository.
    pub applied: Option<Repository>,
}

impl RepositoryPicker {
    /// Clamps the cursor and refreshes the applied repository by provider identity.
    ///
    /// The applied repository stays applied when it is missing from the new rows, so a refresh
    /// can neither retarget a writer nor broaden its scope to every repository.
    pub fn loaded(&mut self) {
        self.cursor = self.cursor.min(self.rows.data.len());
        if let Some(applied) = &mut self.applied
            && let Some(current) = self.rows.data.iter().find(|row| row.id == applied.id)
        {
            applied.clone_from(current);
        }
    }
}

#[derive(Debug, Default)]
pub struct ThreadList {
    pub rows: Loadable<Vec<ThreadSummary>>,
    pub selected: Option<usize>,
    /// Offset of the latest applied page, reused when reloading it.
    pub offset: u64,
    pub next_offset: Option<u64>,
}

impl ThreadList {
    /// Starts a read and clears the old rows: a new query or scope must not show stale results.
    pub fn begin(&mut self) -> u64 {
        self.rows.data.clear();
        self.selected = None;
        self.next_offset = None;
        self.rows.begin()
    }

    pub fn loaded(&mut self, offset: u64, next_offset: Option<u64>) {
        self.offset = offset;
        self.next_offset = next_offset;
        self.selected = (!self.rows.data.is_empty()).then_some(0);
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
    pub fn invalidate(&mut self) {
        self.detail.reset();
        self.scroll = 0;
    }

    pub fn begin(&mut self) -> u64 {
        self.invalidate();
        self.detail.begin()
    }
}

#[derive(Debug, Default)]
pub struct FailureList {
    pub runs: Loadable<Vec<RunFailureSummary>>,
    pub selected: usize,
}

impl FailureList {
    pub fn loaded(&mut self) {
        self.selected = self.selected.min(self.runs.data.len().saturating_sub(1));
    }
}

#[derive(Debug, Default)]
pub struct ClusterList {
    pub rows: Loadable<Vec<ClusterSummary>>,
    pub selected: usize,
}

impl ClusterList {
    pub fn loaded(&mut self) {
        self.selected = self.selected.min(self.rows.data.len().saturating_sub(1));
    }
}

#[derive(Debug, Default)]
pub struct ClusterDetailPane {
    pub detail: Loadable<Option<ClusterDetail>>,
    pub selected_member: usize,
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
            self.selected_member = 0;
        }
        self.detail.begin()
    }

    pub fn loaded(&mut self) {
        let members = self.detail.data.as_ref().map_or(0, |d| d.members.len());
        self.selected_member = self.selected_member.min(members.saturating_sub(1));
    }
}

#[cfg(test)]
mod tests;
