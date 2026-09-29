//! # Coordinate panel results and cross-panel transitions
//!
//! These [`App`] methods connect [`QueryMessage`] delivery with panel owners. Each panel rejects
//! stale generations and applies its own cache, selection, and loading rules. The coordinator
//! forwards current failures to the shared status line without treating failure as an empty
//! success.
//!
//! Starting a discussion-page read also invalidates selected detail; that relationship belongs
//! here because neither panel owns the other. Opening cluster detail resolves a highlighted list
//! row before beginning the member read. Standalone repository, coverage, failed-run, and cluster
//! list reads start directly on their owning panel rather than through extra app wrappers.
//!
//! A finished writer requests fresh repository choices, the current discussion page, coverage,
//! failures, and scoped clusters. If cluster detail is open, its current identity is refreshed too.
//! The writer's durable report and task cleanup remain with the query layer; this file schedules
//! local views of its effects and does not perform archive or provider I/O.
//!
//! Input changes intent and query tasks execute it asynchronously. Rendering only reads these
//! projections (and clamps scroll to its viewport). Keeping these coordination points explicit
//! prevents an obsolete selection from returning through a late successful reply.

use forgesync_core::content::Repository;
use forgesync_engine::sync::SyncProgress;
use forgesync_store::clusters::{ClusterDetail, ClusterPage};
use forgesync_store::reads::{ArchiveStatus, ThreadDetail};

use super::App;
use super::failures::RunFailureSummary;
use super::messages::QueryMessage;
use crate::query::QueryAction;

impl App {
    /// Starts the repository picker and first discussion page on browser entry.
    pub fn initial_actions(&self) -> [QueryAction; 2] {
        [QueryAction::Repositories, self.thread_action(None, 0)]
    }

    /// Schedules reads needed to show archive changes after a writer completes.
    pub fn refresh_after_operation(&mut self) -> Vec<QueryAction> {
        let cluster_detail_id = self
            .cluster_detail_pane
            .data
            .as_ref()
            .map(|detail| detail.cluster.id);
        let mut actions = vec![
            QueryAction::Repositories,
            self.thread_action(self.search_query.clone(), self.thread_list.offset),
            QueryAction::Coverage,
            QueryAction::Failures,
            QueryAction::Clusters {
                repositories: self.repository_scope(),
            },
        ];
        if let Some(id) = cluster_detail_id {
            let generation = self.cluster_detail_pane.begin(id);
            actions.push(QueryAction::ClusterDetail { generation, id });
        }
        actions
    }

    /// Applies a background result only when it belongs to the current query generation.
    pub fn apply(&mut self, message: QueryMessage) {
        match message {
            QueryMessage::Repositories { generation, result } => {
                self.apply_repositories(generation, result)
            }
            QueryMessage::Threads(reply) => self.apply_threads(reply),
            QueryMessage::Detail { generation, result } => self.apply_detail(generation, result),
            QueryMessage::Coverage { generation, result } => {
                self.apply_coverage(generation, result)
            }
            QueryMessage::Failures { generation, result } => {
                self.apply_failures(generation, result)
            }
            QueryMessage::Clusters { generation, result } => {
                self.apply_clusters(generation, result)
            }
            QueryMessage::ClusterDetail { generation, result } => {
                self.apply_cluster_detail(generation, result)
            }
            QueryMessage::OperationProgress {
                generation,
                progress,
            } => self.apply_operation_progress(generation, progress),
            QueryMessage::OperationFinished { generation, result } => {
                self.apply_operation_finished(generation, result)
            }
        }
    }

    /// Updates the repository picker without replacing a newer selection.
    fn apply_repositories(&mut self, generation: u64, result: Result<Vec<Repository>, String>) {
        if let Some(error) = self.repository_picker.apply(generation, result) {
            self.status = Some(error);
        }
    }

    /// Replaces the current thread page and resets its detail selection.
    fn apply_threads(&mut self, reply: super::threads::ThreadReply) {
        if let Some(error) = self.thread_list.apply(reply) {
            self.status = Some(error);
        }
    }

    /// Shows the selected discussion only if it still belongs to the current list.
    fn apply_detail(&mut self, generation: u64, result: Result<Box<ThreadDetail>, String>) {
        if let Some(error) = self.detail_pane.apply(generation, result) {
            self.status = Some(error);
        }
    }

    /// Replaces the coverage view for its latest request generation.
    fn apply_coverage(&mut self, generation: u64, result: Result<Box<ArchiveStatus>, String>) {
        if let Some(error) = self.coverage_panel.apply(generation, result) {
            self.status = Some(error);
        }
    }

    /// Keeps the failure cursor within the latest result set.
    fn apply_failures(&mut self, generation: u64, result: Result<Vec<RunFailureSummary>, String>) {
        if let Some(error) = self.failure_list.apply(generation, result) {
            self.status = Some(error);
        }
    }

    /// Keeps the cluster cursor within the latest generated page.
    fn apply_clusters(&mut self, generation: u64, result: Result<Box<ClusterPage>, String>) {
        if let Some(error) = self.cluster_list.apply(generation, result) {
            self.status = Some(error);
        }
    }

    /// Keeps the selected member valid after a detail refresh.
    fn apply_cluster_detail(
        &mut self,
        generation: u64,
        result: Result<Box<ClusterDetail>, String>,
    ) {
        if let Some(error) = self.cluster_detail_pane.apply(generation, result) {
            self.status = Some(error);
        }
    }

    /// Ignores progress from an action that has already finished or been replaced.
    fn apply_operation_progress(&mut self, generation: u64, progress: SyncProgress) {
        self.operation.update_progress(generation, progress);
    }

    /// Records the terminal operation result and clears transient progress.
    fn apply_operation_finished(&mut self, generation: u64, result: Result<String, String>) {
        if let Some(status) = self.operation.finish(generation, result) {
            self.status = Some(status);
        }
    }

    /// Starts a new thread-list generation and invalidates the selected detail; replies from an
    /// older list must not restore a stale selection after the scope changes.
    pub fn begin_threads(&mut self) -> u64 {
        let generation = self.thread_list.begin();
        self.detail_pane.invalidate();
        generation
    }

    /// Requests the selected cluster only when a valid selection exists.
    pub fn begin_cluster_detail(&mut self) -> Option<(u64, u64)> {
        let cluster_id = self.cluster_list.items.get(self.cluster_list.selected)?.id;
        let generation = self.cluster_detail_pane.begin(cluster_id);
        Some((generation, cluster_id))
    }

    /// Reserves the single active writer slot. `None` leaves existing progress untouched when an
    /// operation is already running; the generation tags later completion messages.
    pub fn begin_operation(&mut self, label: &str) -> Option<u64> {
        let generation = self.operation.begin(label)?;
        self.status = Some(format!("Starting {label}…"));
        Some(generation)
    }
}
