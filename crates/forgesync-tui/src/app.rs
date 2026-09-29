use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use forgesync_core::content::Repository;
use forgesync_core::identity::RunId;
use forgesync_engine::{
    ArchiveStatus, ClusterDetail, ClusterPage, ClusterSummary, RepositorySelector, RunStatus,
    SyncProgress, ThreadDetail, ThreadPage, ThreadSelector, ThreadSummary,
};

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
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
    use forgesync_core::identity::{
        GitHubHost, ProviderId, RepositoryId, RunId, ThreadId, ThreadNumber,
    };
    use forgesync_core::provider_data::ProviderData;
    use forgesync_core::timestamp::UtcTimestamp;
    use forgesync_engine::{
        ClusterDetail, ClusterLifecycle, ClusterMember, ClusterMemberRole, ClusterMemberState,
        ClusterSummary, SyncProgress, SyncProgressStatus, ThreadSummary,
    };
    use forgesync_store::ThreadPage;

    use super::{App, Focus, QueryAction, QueryMessage, Screen};

    #[test]
    fn keyboard_input_remains_available_while_queries_are_pending() {
        let mut app = App {
            threads_loading: true,
            ..App::default()
        };

        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.focus, Focus::Threads);
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.quit);
    }

    #[test]
    fn stale_thread_result_does_not_replace_current_query_state() {
        let mut app = App {
            thread_generation: 2,
            threads_loading: true,
            ..App::default()
        };
        app.apply(QueryMessage::Threads {
            generation: 1,
            offset: 0,
            result: Ok(Box::new(ThreadPage {
                items: Vec::new(),
                next_offset: None,
                coverage: Vec::new(),
            })),
        });

        assert!(app.threads_loading);
        assert_eq!(app.thread_generation, 2);
        assert!(app.threads.is_empty());
    }

    #[test]
    fn search_keys_build_a_local_query_until_enter() {
        let mut app = App {
            searching: true,
            ..App::default()
        };
        for character in "tui search".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }
        assert!(app.search_query.is_none());

        let actions = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.search_query.as_deref(), Some("tui search"));
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn repository_picker_applies_the_highlighted_repository() {
        let repository = Repository {
            id: RepositoryId::new(
                GitHubHost::parse("github.com").expect("host"),
                ProviderId::new("41").expect("repository ID"),
            ),
            owner: "owner".to_owned(),
            name: "repo".to_owned(),
            full_name: "owner/repo".to_owned(),
            default_branch: None,
            updated_at: None,
            provider_data: ProviderData::new(),
        };
        let mut app = App {
            repositories: vec![repository],
            repository_cursor: 1,
            ..App::default()
        };

        let actions = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(app.applied_repository, Some(0));
        assert!(matches!(app.screen, Screen::Browser));
        match actions.as_slice() {
            [
                QueryAction::Threads {
                    repositories,
                    offset: 0,
                    ..
                },
            ] => {
                assert_eq!(repositories.len(), 1);
                assert_eq!(repositories[0].as_url(), "https://github.com/owner/repo");
            }
            _ => panic!("expected a repository-scoped thread query"),
        }
    }

    #[test]
    fn opening_keyword_search_returns_to_the_browser() {
        let mut app = App {
            screen: Screen::Failures,
            ..App::default()
        };

        app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));

        assert!(app.searching);
        assert_eq!(app.screen, Screen::Browser);
    }

    #[test]
    fn maintainer_keys_target_the_selected_cluster_member() {
        let mut app = App {
            screen: Screen::ClusterDetail,
            cluster_detail: Some(sample_cluster_detail()),
            ..App::default()
        };

        let exclude = app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert!(matches!(
            exclude.as_slice(),
            [QueryAction::SetClusterMemberExcluded {
                id: 17,
                excluded: true,
                ..
            }]
        ));

        let canonical = app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert!(matches!(
            canonical.as_slice(),
            [QueryAction::SetCanonicalClusterMember { id: 17, .. }]
        ));
    }

    #[test]
    fn writer_actions_use_current_repository_scope() {
        let repository = sample_repository();
        let mut app = App {
            repositories: vec![repository],
            applied_repository: Some(0),
            ..App::default()
        };

        let sync = app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        assert!(matches!(
            sync.as_slice(),
            [QueryAction::Sync { repositories }] if repositories.len() == 1
        ));
        let refresh = app.handle_key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::NONE));
        assert!(matches!(
            refresh.as_slice(),
            [QueryAction::Refresh { repositories }] if repositories.len() == 1
        ));
    }

    #[test]
    fn cluster_dismiss_and_selected_run_retry_use_the_current_selection() {
        let detail = sample_cluster_detail();
        let mut app = App {
            screen: Screen::Clusters,
            clusters: vec![detail.cluster],
            failures: vec![super::RunFailureSummary {
                id: 23,
                status: super::RunStatus::Failed,
                entries: vec!["owner/repo: threads failed".to_owned()],
            }],
            ..App::default()
        };
        let dismiss = app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert!(matches!(
            dismiss.as_slice(),
            [QueryAction::DismissCluster {
                id: 17,
                dismissed: true,
            }]
        ));

        app.screen = Screen::Failures;
        let retry = app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
        assert!(matches!(
            retry.as_slice(),
            [QueryAction::Retry(run_id)] if *run_id == RunId::new(23).expect("run ID")
        ));
    }

    #[test]
    fn quit_cancels_active_action_and_failed_result_stays_visible() {
        let mut app = App {
            operation_generation: 4,
            operation_busy: true,
            operation_label: Some("sync".to_owned()),
            ..App::default()
        };

        let action = app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(matches!(action.as_slice(), [QueryAction::CancelOperation]));
        assert!(!app.quit);

        app.apply(QueryMessage::OperationFinished {
            generation: 4,
            result: Err("archive writer lease is held".to_owned()),
        });
        assert!(!app.operation_busy);
        assert_eq!(
            app.status.as_deref(),
            Some("Failed: archive writer lease is held")
        );
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.quit);
    }

    #[test]
    fn stale_operation_progress_cannot_replace_current_progress() {
        let mut app = App {
            operation_generation: 3,
            operation_busy: true,
            ..App::default()
        };
        let progress = SyncProgress {
            run_id: RunId::new(1).expect("positive run ID"),
            completed_jobs: 1,
            total_jobs: 2,
            threads_seen: 3,
            comments_seen: 4,
            pull_request_metadata_seen: 5,
            reviews_seen: 6,
            review_threads_seen: 7,
            repository: Some("https://github.com/owner/repo".to_owned()),
            status: SyncProgressStatus::InProgress,
        };

        app.apply(QueryMessage::OperationProgress {
            generation: 2,
            progress,
        });
        assert!(app.operation_progress.is_none());
    }

    fn sample_repository() -> Repository {
        Repository {
            id: RepositoryId::new(
                GitHubHost::parse("github.com").expect("host"),
                ProviderId::new("41").expect("repository ID"),
            ),
            owner: "owner".to_owned(),
            name: "repo".to_owned(),
            full_name: "owner/repo".to_owned(),
            default_branch: Some("main".to_owned()),
            updated_at: None,
            provider_data: ProviderData::new(),
        }
    }

    fn sample_cluster_detail() -> ClusterDetail {
        let repository = sample_repository();
        let timestamp = UtcTimestamp::parse("2026-09-29T00:00:00Z").expect("timestamp");
        let discussion = Discussion {
            id: ThreadId::new(
                repository.id.clone(),
                ProviderId::new("1001").expect("thread ID"),
                ThreadNumber::new(7).expect("thread number"),
            ),
            kind: ThreadKind::Issue,
            state: SourceState::Open,
            title: "Selected neighbor".to_owned(),
            body: None,
            html_url: None,
            created_at: timestamp,
            updated_at: timestamp,
            closed_at: None,
            labels: Vec::new(),
            assignees: Vec::new(),
            provider_data: ProviderData::new(),
        };
        ClusterDetail {
            cluster: ClusterSummary {
                id: 17,
                repository: repository.clone(),
                title: "Cluster title".to_owned(),
                lifecycle: ClusterLifecycle::Active,
                dismissed: false,
                dismissal_reason: None,
                representative: None,
                active_member_count: 1,
                excluded_member_count: 0,
                last_run_id: Some(2),
                updated_at: timestamp,
            },
            members: vec![ClusterMember {
                summary: ThreadSummary {
                    repository,
                    discussion,
                    coverage: Vec::new(),
                },
                role: ClusterMemberRole::Representative,
                state: ClusterMemberState::Active,
                score_to_representative: Some(1.0),
            }],
        }
    }
}
