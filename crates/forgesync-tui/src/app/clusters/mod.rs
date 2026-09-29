//! # Scoped cluster choices and selected member detail
//!
//! [`ClusterList`] owns scoped cluster rows, highlight, and refresh generation.
//! [`ClusterDetailPane`] owns one cluster's member projection, selected member, and its independent
//! detail read. The app coordinates opening a highlighted cluster; query tasks own archive access
//! and local decisions.
//!
//! List refresh retains prior rows while pending or failed and clamps selection on success.
//! Detail refresh retains members only when refreshing the same cluster. Opening another cluster
//! removes old members immediately, so their canonical/exclusion actions cannot target the previous
//! cluster while the new read is pending. A new selection starts at its first available member.
//!
//! Stale replies change neither loading nor visible data. Current failures are returned to the
//! app's status line and shown before cached data by rendering. These owners do not optimistically
//! apply local decisions: member roles and exclusions come from the archive after the writer ends.

use forgesync_store::clusters::{ClusterDetail, ClusterPage, ClusterSummary};

/// Cluster choices and selection for the applied repository scope.
#[derive(Debug, Default)]
pub struct ClusterList {
    /// Last successful rows in engine query order, retained during refresh.
    pub items: Vec<ClusterSummary>,
    /// Highlighted row; zero is also the empty-list sentinel and must be checked against `items`.
    pub selected: usize,
    /// Most recently started scoped list read.
    pub generation: u64,
    /// Whether the current list read is pending.
    pub loading: bool,
    /// Current safe list-read failure, cleared before another request or successful replacement.
    pub error: Option<String>,
}

/// One loaded cluster and its member selection, independently refreshed from the list.
#[derive(Debug, Default)]
pub struct ClusterDetailPane {
    /// Last successful projection for the selected cluster, absent while opening a different one.
    pub data: Option<ClusterDetail>,
    /// Highlighted member; checked against the loaded member list before constructing a decision.
    pub selected_member: usize,
    /// Most recently started detail read, advanced for both opening and same-cluster refresh.
    pub generation: u64,
    /// Whether the selected cluster's current read is pending.
    pub loading: bool,
    /// Safe current detail-read failure, shown before any retained same-cluster data.
    pub error: Option<String>,
}

impl ClusterList {
    /// Starts a scoped refresh while retaining existing choices and highlight.
    pub fn begin(&mut self) -> u64 {
        self.generation += 1;
        self.loading = true;
        self.error = None;
        self.generation
    }

    /// Applies only the latest scoped page and returns its failure for the app's status line.
    pub fn apply(
        &mut self,
        generation: u64,
        result: Result<Box<ClusterPage>, String>,
    ) -> Option<String> {
        if generation != self.generation {
            return None;
        }
        self.loading = false;
        match result {
            Ok(page) => self.replace(*page),
            Err(error) => self.fail(error),
        }
    }

    /// Replaces choices and clamps the highlighted row to the new page bounds.
    fn replace(&mut self, page: ClusterPage) -> Option<String> {
        self.items = page.items;
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
        self.error = None;
        None
    }

    /// Retains previous choices while exposing a current read failure.
    fn fail(&mut self, error: String) -> Option<String> {
        self.error = Some(error.clone());
        Some(error)
    }
}

impl ClusterDetailPane {
    /// Starts a member read, retaining cached data only when it belongs to the requested cluster.
    ///
    /// A different or absent loaded cluster clears members and selection immediately. This keeps
    /// keyboard decisions from referring to old members while the selected cluster is loading.
    pub fn begin(&mut self, cluster_id: u64) -> u64 {
        if self
            .data
            .as_ref()
            .is_none_or(|detail| detail.cluster.id != cluster_id)
        {
            self.data = None;
            self.selected_member = 0;
        }
        self.generation += 1;
        self.loading = true;
        self.error = None;
        self.generation
    }

    /// Applies current members and returns a safe failure without altering a newer selection.
    pub fn apply(
        &mut self,
        generation: u64,
        result: Result<Box<ClusterDetail>, String>,
    ) -> Option<String> {
        if generation != self.generation {
            return None;
        }
        self.loading = false;
        match result {
            Ok(detail) => self.replace(*detail),
            Err(error) => self.fail(error),
        }
    }

    /// Stores current membership and clamps the selected member to its actual bounds.
    fn replace(&mut self, detail: ClusterDetail) -> Option<String> {
        self.selected_member = self
            .selected_member
            .min(detail.members.len().saturating_sub(1));
        self.data = Some(detail);
        self.error = None;
        None
    }

    /// Retains only the currently selected cluster's cache while showing its read failure.
    fn fail(&mut self, error: String) -> Option<String> {
        self.error = Some(error.clone());
        Some(error)
    }
}

#[cfg(test)]
mod tests;
