//! # Apply query results and maintain selection
//!
//! State methods receive `QueryMessage` values, update loaded pages or detail, and keep selection
//! valid as data changes. This is the point where asynchronous archive results become visible app
//! state.
//!
//! Input asks for work and `query` performs it; `view` only reads the resulting state.
//! Centralizing the application step prevents stale results or failed operations from being
//! silently treated as a successful empty page.

use super::{App, QueryAction, QueryMessage};

impl App {
    /// Starts the repository picker and first discussion page on browser entry.
    pub fn initial_actions(&self) -> [QueryAction; 2] {
        [QueryAction::Repositories, self.thread_action(None, 0)]
    }

    /// Schedules reads needed to show archive changes after a writer completes.
    pub fn refresh_after_operation(&mut self) -> Vec<QueryAction> {
        let cluster_detail_id = self.cluster_detail.as_ref().map(|detail| detail.cluster.id);
        let mut actions = vec![
            QueryAction::Repositories,
            self.thread_action(self.search_query.clone(), self.page_offset),
            QueryAction::Coverage,
            QueryAction::Failures,
            QueryAction::Clusters {
                repositories: self.repository_scope(),
            },
        ];
        if let Some(id) = cluster_detail_id {
            self.cluster_detail_generation += 1;
            self.cluster_detail_loading = true;
            self.cluster_detail_error = None;
            actions.push(QueryAction::ClusterDetail {
                generation: self.cluster_detail_generation,
                id,
            });
        }
        actions
    }

    /// Applies a background result only when it belongs to the current query generation.
    pub fn apply(&mut self, message: QueryMessage) {
        match message {
            QueryMessage::Repositories { generation, result } => {
                self.apply_repositories(generation, result)
            }
            QueryMessage::Threads {
                generation,
                offset,
                result,
            } => self.apply_threads(generation, offset, result),
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
    fn apply_repositories(
        &mut self,
        generation: u64,
        result: Result<Vec<super::Repository>, String>,
    ) {
        if generation != self.repository_generation {
            return;
        }
        self.repositories_loading = false;
        match result {
            Ok(repositories) => {
                self.repository_error = None;
                self.repositories = repositories;
                self.repository_cursor = self.repository_cursor.min(self.repositories.len());
            }
            Err(error) => {
                self.repository_error = Some(error.clone());
                self.status = Some(error);
            }
        }
    }

    /// Replaces the current thread page and resets its detail selection.
    fn apply_threads(
        &mut self,
        generation: u64,
        offset: u64,
        result: Result<Box<super::ThreadPage>, String>,
    ) {
        if generation != self.thread_generation {
            return;
        }
        self.threads_loading = false;
        self.page_offset = offset;
        match result {
            Ok(page) => {
                let page = *page;
                self.thread_error = None;
                self.threads = page.items;
                self.next_offset = page.next_offset;
                self.selected_thread = (!self.threads.is_empty()).then_some(0);
            }
            Err(error) => {
                self.thread_error = Some(error.clone());
                self.threads.clear();
                self.next_offset = None;
                self.selected_thread = None;
                self.status = Some(error);
            }
        }
    }

    /// Shows the selected discussion only if it still belongs to the current list.
    fn apply_detail(&mut self, generation: u64, result: Result<Box<super::ThreadDetail>, String>) {
        if generation != self.detail_generation {
            return;
        }
        self.detail_loading = false;
        match result {
            Ok(detail) => {
                self.detail_error = None;
                self.detail = Some(*detail);
                self.detail_scroll = 0;
            }
            Err(error) => {
                self.detail_error = Some(error.clone());
                self.detail = None;
                self.status = Some(error);
            }
        }
    }

    /// Replaces the coverage view for its latest request generation.
    fn apply_coverage(
        &mut self,
        generation: u64,
        result: Result<Box<super::ArchiveStatus>, String>,
    ) {
        if generation != self.coverage_generation {
            return;
        }
        self.coverage_loading = false;
        match result {
            Ok(coverage) => {
                self.coverage_error = None;
                self.coverage = Some(*coverage);
            }
            Err(error) => {
                self.coverage_error = Some(error.clone());
                self.status = Some(error);
            }
        }
    }

    /// Keeps the failure cursor within the latest result set.
    fn apply_failures(
        &mut self,
        generation: u64,
        result: Result<Vec<super::RunFailureSummary>, String>,
    ) {
        if generation != self.failures_generation {
            return;
        }
        self.failures_loading = false;
        match result {
            Ok(failures) => {
                self.failures_error = None;
                self.failures = failures;
                self.selected_failure = self
                    .selected_failure
                    .min(self.failures.len().saturating_sub(1));
            }
            Err(error) => {
                self.failures_error = Some(error.clone());
                self.status = Some(error);
            }
        }
    }

    /// Keeps the cluster cursor within the latest generated page.
    fn apply_clusters(&mut self, generation: u64, result: Result<Box<super::ClusterPage>, String>) {
        if generation != self.clusters_generation {
            return;
        }
        self.clusters_loading = false;
        match result {
            Ok(page) => {
                self.clusters_error = None;
                self.clusters = page.items;
                self.selected_cluster = self
                    .selected_cluster
                    .min(self.clusters.len().saturating_sub(1));
            }
            Err(error) => {
                self.clusters_error = Some(error.clone());
                self.status = Some(error);
            }
        }
    }

    /// Keeps the selected member valid after a detail refresh.
    fn apply_cluster_detail(
        &mut self,
        generation: u64,
        result: Result<Box<super::ClusterDetail>, String>,
    ) {
        if generation != self.cluster_detail_generation {
            return;
        }
        self.cluster_detail_loading = false;
        match result {
            Ok(detail) => {
                self.cluster_detail_error = None;
                self.cluster_detail = Some(*detail);
                self.selected_cluster_member = self.selected_cluster_member.min(
                    self.cluster_detail
                        .as_ref()
                        .map_or(0, |detail| detail.members.len().saturating_sub(1)),
                );
            }
            Err(error) => {
                self.cluster_detail_error = Some(error.clone());
                self.status = Some(error);
            }
        }
    }

    /// Ignores progress from an action that has already finished or been replaced.
    fn apply_operation_progress(&mut self, generation: u64, progress: super::SyncProgress) {
        if generation == self.operation_generation && self.operation_busy {
            self.operation_progress = Some(progress);
        }
    }

    /// Records the terminal operation result and clears transient progress.
    fn apply_operation_finished(&mut self, generation: u64, result: Result<String, String>) {
        if generation != self.operation_generation {
            return;
        }
        self.operation_busy = false;
        self.operation_label = None;
        self.operation_progress = None;
        self.status = Some(match result {
            Ok(summary) => summary,
            Err(error) => format!("Failed: {error}"),
        });
    }

    /// Advances repository generation so older read results cannot replace this request.
    pub fn begin_repositories(&mut self) -> u64 {
        self.repository_generation += 1;
        self.repositories_loading = true;
        self.repository_error = None;
        self.repository_generation
    }

    /// Starts a new thread-list generation and invalidates the selected detail; replies from an
    /// older list must not restore a stale selection after the scope changes.
    pub fn begin_threads(&mut self) -> u64 {
        self.thread_generation += 1;
        self.threads_loading = true;
        self.thread_error = None;
        self.threads.clear();
        self.selected_thread = None;
        self.next_offset = None;
        self.invalidate_detail();
        self.thread_generation
    }

    /// Invalidates an older detail request when the selected discussion changes.
    pub fn begin_detail(&mut self) -> u64 {
        self.detail_generation += 1;
        self.detail_loading = true;
        self.detail_error = None;
        self.detail = None;
        self.detail_scroll = 0;
        self.detail_generation
    }

    /// Starts a new coverage generation and clears the previous loading error.
    pub fn begin_coverage(&mut self) -> u64 {
        self.coverage_generation += 1;
        self.coverage_loading = true;
        self.coverage_error = None;
        self.coverage_generation
    }

    /// Starts a new failure-list generation for the current archive.
    pub fn begin_failures(&mut self) -> u64 {
        self.failures_generation += 1;
        self.failures_loading = true;
        self.failures_error = None;
        self.failures_generation
    }

    /// Starts a new cluster-list generation for the current scope.
    pub fn begin_clusters(&mut self) -> u64 {
        self.clusters_generation += 1;
        self.clusters_loading = true;
        self.clusters_error = None;
        self.clusters_generation
    }

    /// Requests the selected cluster only when a valid selection exists.
    pub fn begin_cluster_detail(&mut self) -> Option<(u64, u64)> {
        let cluster_id = self.clusters.get(self.selected_cluster)?.id;
        self.cluster_detail_generation += 1;
        self.cluster_detail_loading = true;
        self.cluster_detail_error = None;
        Some((self.cluster_detail_generation, cluster_id))
    }

    /// Reserves the single active writer slot. `None` leaves existing progress untouched when an
    /// operation is already running; the generation tags later completion messages.
    pub fn begin_operation(&mut self, label: &str) -> Option<u64> {
        if self.operation_busy {
            return None;
        }
        self.operation_generation += 1;
        self.operation_busy = true;
        self.operation_label = Some(label.to_owned());
        self.operation_progress = None;
        self.status = Some(format!("Starting {label}…"));
        Some(self.operation_generation)
    }
}
