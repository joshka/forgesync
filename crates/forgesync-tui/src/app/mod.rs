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

impl App {
    pub(crate) fn initial_actions(&self) -> [QueryAction; 2] {
        [QueryAction::Repositories, self.thread_action(None, 0)]
    }

    pub(crate) fn refresh_after_operation(&mut self) -> Vec<QueryAction> {
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

    pub(crate) fn apply(&mut self, message: QueryMessage) {
        match message {
            QueryMessage::Repositories { generation, result } => {
                if generation != self.repository_generation {
                    return;
                }
                self.repositories_loading = false;
                match result {
                    Ok(repositories) => {
                        self.repository_error = None;
                        self.repositories = repositories;
                        self.repository_cursor =
                            self.repository_cursor.min(self.repositories.len());
                    }
                    Err(error) => {
                        self.repository_error = Some(error.clone());
                        self.status = Some(error);
                    }
                }
            }
            QueryMessage::Threads {
                generation,
                offset,
                result,
            } => {
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
            QueryMessage::Detail { generation, result } => {
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
            QueryMessage::Coverage { generation, result } => {
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
            QueryMessage::Failures { generation, result } => {
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
            QueryMessage::Clusters { generation, result } => {
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
            QueryMessage::ClusterDetail { generation, result } => {
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
            QueryMessage::OperationProgress {
                generation,
                progress,
            } => {
                if generation == self.operation_generation && self.operation_busy {
                    self.operation_progress = Some(progress);
                }
            }
            QueryMessage::OperationFinished { generation, result } => {
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
        }
    }

    pub(crate) fn begin_repositories(&mut self) -> u64 {
        self.repository_generation += 1;
        self.repositories_loading = true;
        self.repository_error = None;
        self.repository_generation
    }

    pub(crate) fn begin_threads(&mut self) -> u64 {
        self.thread_generation += 1;
        self.threads_loading = true;
        self.thread_error = None;
        self.threads.clear();
        self.selected_thread = None;
        self.next_offset = None;
        self.invalidate_detail();
        self.thread_generation
    }

    pub(crate) fn begin_detail(&mut self) -> u64 {
        self.detail_generation += 1;
        self.detail_loading = true;
        self.detail_error = None;
        self.detail = None;
        self.detail_scroll = 0;
        self.detail_generation
    }

    pub(crate) fn begin_coverage(&mut self) -> u64 {
        self.coverage_generation += 1;
        self.coverage_loading = true;
        self.coverage_error = None;
        self.coverage_generation
    }

    pub(crate) fn begin_failures(&mut self) -> u64 {
        self.failures_generation += 1;
        self.failures_loading = true;
        self.failures_error = None;
        self.failures_generation
    }

    pub(crate) fn begin_clusters(&mut self) -> u64 {
        self.clusters_generation += 1;
        self.clusters_loading = true;
        self.clusters_error = None;
        self.clusters_generation
    }

    pub(crate) fn begin_cluster_detail(&mut self) -> Option<(u64, u64)> {
        let cluster_id = self.clusters.get(self.selected_cluster)?.id;
        self.cluster_detail_generation += 1;
        self.cluster_detail_loading = true;
        self.cluster_detail_error = None;
        Some((self.cluster_detail_generation, cluster_id))
    }

    pub(crate) fn begin_operation(&mut self, label: &str) -> Option<u64> {
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

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> Vec<QueryAction> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return self.request_quit();
        }
        if self.searching {
            return self.handle_search_key(key.code);
        }

        match key.code {
            KeyCode::Char('q') => return self.request_quit(),
            KeyCode::Char('c') => {
                self.screen = Screen::Coverage;
                self.status = None;
                return vec![QueryAction::Coverage];
            }
            KeyCode::Char('f') => {
                self.screen = Screen::Failures;
                self.status = None;
                return vec![QueryAction::Failures];
            }
            KeyCode::Char('g') => {
                self.screen = Screen::Clusters;
                self.status = None;
                return vec![QueryAction::Clusters {
                    repositories: self.repository_scope(),
                }];
            }
            KeyCode::Char('s') => {
                return vec![QueryAction::Sync {
                    repositories: self.repository_scope(),
                }];
            }
            KeyCode::Char('R') => {
                return vec![QueryAction::Refresh {
                    repositories: self.repository_scope(),
                }];
            }
            KeyCode::Char('t') if self.screen == Screen::Failures => {
                if let Some(run) = self.failures.get(self.selected_failure)
                    && let Ok(run_id) = RunId::new(run.id)
                {
                    return vec![QueryAction::Retry(run_id)];
                }
            }
            KeyCode::Char('d') if self.screen == Screen::Clusters => {
                if let Some(cluster) = self.clusters.get(self.selected_cluster) {
                    return vec![QueryAction::DismissCluster {
                        id: cluster.id,
                        dismissed: !cluster.dismissed,
                    }];
                }
            }
            KeyCode::Char('d') if self.screen == Screen::ClusterDetail => {
                if let Some(detail) = &self.cluster_detail {
                    return vec![QueryAction::DismissCluster {
                        id: detail.cluster.id,
                        dismissed: !detail.cluster.dismissed,
                    }];
                }
            }
            KeyCode::Char('e' | 'i') if self.screen == Screen::ClusterDetail => {
                if let Some(member) = self
                    .cluster_detail
                    .as_ref()
                    .and_then(|detail| detail.members.get(self.selected_cluster_member))
                    && let Some(detail) = &self.cluster_detail
                {
                    let reference = ThreadSelector::new(
                        RepositorySelector::from_repository(&member.summary.repository),
                        member.summary.discussion.id.number(),
                    );
                    return vec![QueryAction::SetClusterMemberExcluded {
                        id: detail.cluster.id,
                        reference,
                        excluded: key.code == KeyCode::Char('e'),
                    }];
                }
            }
            KeyCode::Char('k') if self.screen == Screen::ClusterDetail => {
                if let Some(member) = self
                    .cluster_detail
                    .as_ref()
                    .and_then(|detail| detail.members.get(self.selected_cluster_member))
                    && let Some(detail) = &self.cluster_detail
                {
                    let reference = ThreadSelector::new(
                        RepositorySelector::from_repository(&member.summary.repository),
                        member.summary.discussion.id.number(),
                    );
                    return vec![QueryAction::SetCanonicalClusterMember {
                        id: detail.cluster.id,
                        reference,
                    }];
                }
            }
            KeyCode::Char('/') => {
                self.screen = Screen::Browser;
                self.searching = true;
                self.search_input = self.search_query.clone().unwrap_or_default();
            }
            KeyCode::Char('r') if self.screen == Screen::Browser => {
                self.status = None;
                return vec![self.thread_action(self.search_query.clone(), self.page_offset)];
            }
            KeyCode::Char('n') if self.screen == Screen::Browser => {
                if let Some(offset) = self.next_offset {
                    return vec![self.thread_action(self.search_query.clone(), offset)];
                }
            }
            KeyCode::Char('p') if self.screen == Screen::Browser => {
                if self.page_offset > 0 {
                    let offset = self.page_offset.saturating_sub(u64::from(PAGE_SIZE));
                    return vec![self.thread_action(self.search_query.clone(), offset)];
                }
            }
            KeyCode::Tab if self.screen == Screen::Browser => self.focus = self.next_focus(),
            KeyCode::BackTab if self.screen == Screen::Browser => {
                self.focus = self.previous_focus()
            }
            KeyCode::Up | KeyCode::Char('k') if self.screen == Screen::Browser => {
                self.move_selection(-1);
            }
            KeyCode::Down | KeyCode::Char('j') if self.screen == Screen::Browser => {
                self.move_selection(1);
            }
            KeyCode::Up | KeyCode::Char('k') if self.screen == Screen::Clusters => {
                self.selected_cluster = move_index(
                    self.selected_cluster,
                    self.clusters.len().saturating_sub(1),
                    -1,
                );
            }
            KeyCode::Down | KeyCode::Char('j') if self.screen == Screen::Clusters => {
                if !self.clusters.is_empty() {
                    self.selected_cluster = move_index(
                        self.selected_cluster,
                        self.clusters.len().saturating_sub(1),
                        1,
                    );
                }
            }
            KeyCode::Up | KeyCode::Char('k') if self.screen == Screen::ClusterDetail => {
                self.selected_cluster_member = move_index(
                    self.selected_cluster_member,
                    self.cluster_detail
                        .as_ref()
                        .map_or(0, |detail| detail.members.len().saturating_sub(1)),
                    -1,
                );
            }
            KeyCode::Down | KeyCode::Char('j') if self.screen == Screen::ClusterDetail => {
                if let Some(detail) = &self.cluster_detail
                    && !detail.members.is_empty()
                {
                    self.selected_cluster_member = move_index(
                        self.selected_cluster_member,
                        detail.members.len().saturating_sub(1),
                        1,
                    );
                }
            }
            KeyCode::Up | KeyCode::Char('k') if self.screen == Screen::Failures => {
                self.selected_failure = move_index(
                    self.selected_failure,
                    self.failures.len().saturating_sub(1),
                    -1,
                );
            }
            KeyCode::Down | KeyCode::Char('j') if self.screen == Screen::Failures => {
                if !self.failures.is_empty() {
                    self.selected_failure = move_index(
                        self.selected_failure,
                        self.failures.len().saturating_sub(1),
                        1,
                    );
                }
            }
            KeyCode::PageUp if self.screen == Screen::Browser => self.move_page(-1),
            KeyCode::PageDown if self.screen == Screen::Browser => self.move_page(1),
            KeyCode::Home if self.screen == Screen::Browser => self.move_to_edge(false),
            KeyCode::End if self.screen == Screen::Browser => self.move_to_edge(true),
            KeyCode::Enter if self.screen == Screen::Browser => return self.select(),
            KeyCode::Enter if self.screen == Screen::Clusters => {
                if let Some((generation, id)) = self.begin_cluster_detail() {
                    return vec![QueryAction::ClusterDetail { generation, id }];
                }
            }
            KeyCode::Esc => {
                if self.screen == Screen::ClusterDetail {
                    self.screen = Screen::Clusters;
                } else if self.screen != Screen::Browser {
                    self.screen = Screen::Browser;
                } else if self.search_query.is_some() {
                    self.search_query = None;
                    return vec![self.thread_action(None, 0)];
                } else if self.focus == Focus::Detail {
                    self.focus = Focus::Threads;
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_search_key(&mut self, code: KeyCode) -> Vec<QueryAction> {
        match code {
            KeyCode::Esc => {
                self.searching = false;
                self.search_input.clear();
            }
            KeyCode::Enter => {
                self.searching = false;
                let query = self.search_input.trim().to_owned();
                self.search_query = (!query.is_empty()).then_some(query);
                self.search_input.clear();
                return vec![self.thread_action(self.search_query.clone(), 0)];
            }
            KeyCode::Backspace => {
                self.search_input.pop();
            }
            KeyCode::Char(character) => self.search_input.push(character),
            _ => {}
        }
        Vec::new()
    }

    fn next_focus(&self) -> Focus {
        match self.focus {
            Focus::Repositories => Focus::Threads,
            Focus::Threads => Focus::Detail,
            Focus::Detail => Focus::Repositories,
        }
    }

    fn previous_focus(&self) -> Focus {
        match self.focus {
            Focus::Repositories => Focus::Detail,
            Focus::Threads => Focus::Repositories,
            Focus::Detail => Focus::Threads,
        }
    }

    fn move_selection(&mut self, direction: i8) {
        match self.focus {
            Focus::Repositories => {
                let max = self.repositories.len();
                self.repository_cursor = move_index(self.repository_cursor, max, direction);
            }
            Focus::Threads => {
                let max = self.threads.len().saturating_sub(1);
                if !self.threads.is_empty() {
                    let next = move_index(self.selected_thread.unwrap_or(0), max, direction);
                    self.selected_thread = Some(next);
                    self.invalidate_detail();
                }
            }
            Focus::Detail => {
                self.detail_scroll = if direction < 0 {
                    self.detail_scroll.saturating_sub(1)
                } else {
                    self.detail_scroll.saturating_add(1)
                };
            }
        }
    }

    fn move_page(&mut self, direction: i8) {
        match self.focus {
            Focus::Detail => {
                self.detail_scroll = if direction < 0 {
                    self.detail_scroll.saturating_sub(10)
                } else {
                    self.detail_scroll.saturating_add(10)
                };
            }
            Focus::Repositories => {
                let max = self.repositories.len();
                let step = 10usize;
                self.repository_cursor = if direction < 0 {
                    self.repository_cursor.saturating_sub(step)
                } else {
                    self.repository_cursor.saturating_add(step).min(max)
                };
            }
            Focus::Threads if !self.threads.is_empty() => {
                let max = self.threads.len().saturating_sub(1);
                let current = self.selected_thread.unwrap_or(0);
                let next = if direction < 0 {
                    current.saturating_sub(10)
                } else {
                    current.saturating_add(10).min(max)
                };
                self.selected_thread = Some(next);
                self.invalidate_detail();
            }
            Focus::Threads => {}
        }
    }

    fn move_to_edge(&mut self, end: bool) {
        match self.focus {
            Focus::Repositories => {
                self.repository_cursor = if end { self.repositories.len() } else { 0 };
            }
            Focus::Threads if !self.threads.is_empty() => {
                self.selected_thread = Some(if end { self.threads.len() - 1 } else { 0 });
                self.invalidate_detail();
            }
            Focus::Threads | Focus::Detail => {
                self.detail_scroll = if end { u16::MAX } else { 0 };
            }
        }
    }

    fn select(&mut self) -> Vec<QueryAction> {
        match self.focus {
            Focus::Repositories => {
                let selected = self.repository_cursor.checked_sub(1);
                if selected != self.applied_repository {
                    self.status = None;
                    self.applied_repository = selected;
                    self.page_offset = 0;
                    self.detail = None;
                    return vec![self.thread_action(self.search_query.clone(), 0)];
                }
            }
            Focus::Threads => {
                let Some(summary) = self
                    .selected_thread
                    .and_then(|index| self.threads.get(index))
                else {
                    return Vec::new();
                };
                let selector = ThreadSelector::new(
                    RepositorySelector::from_repository(&summary.repository),
                    summary.discussion.id.number(),
                );
                self.focus = Focus::Detail;
                return vec![QueryAction::Detail(selector)];
            }
            Focus::Detail => {}
        }
        Vec::new()
    }

    fn thread_action(&self, query: Option<String>, offset: u64) -> QueryAction {
        let repositories = self
            .applied_repository
            .and_then(|index| self.repositories.get(index))
            .map(RepositorySelector::from_repository)
            .into_iter()
            .collect();
        QueryAction::Threads {
            query,
            repositories,
            offset,
        }
    }

    fn repository_scope(&self) -> Vec<RepositorySelector> {
        self.applied_repository
            .and_then(|index| self.repositories.get(index))
            .map(RepositorySelector::from_repository)
            .into_iter()
            .collect()
    }

    fn request_quit(&mut self) -> Vec<QueryAction> {
        if self.operation_busy {
            self.status = Some("Cancellation requested; waiting for the active action…".to_owned());
            vec![QueryAction::CancelOperation]
        } else {
            self.quit = true;
            Vec::new()
        }
    }

    fn invalidate_detail(&mut self) {
        self.detail_generation += 1;
        self.detail = None;
        self.detail_loading = false;
        self.detail_error = None;
        self.detail_scroll = 0;
    }
}

fn move_index(current: usize, max: usize, direction: i8) -> usize {
    if direction < 0 {
        current.saturating_sub(1)
    } else {
        current.saturating_add(1).min(max)
    }
}

#[cfg(test)]
mod tests;
