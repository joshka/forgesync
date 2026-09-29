//! Interpret keys as navigation and maintainer actions.

use super::*;

impl App {
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

    pub(super) fn thread_action(&self, query: Option<String>, offset: u64) -> QueryAction {
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

    pub(super) fn repository_scope(&self) -> Vec<RepositorySelector> {
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

    pub(super) fn invalidate_detail(&mut self) {
        self.detail_generation += 1;
        self.detail = None;
        self.detail_loading = false;
        self.detail_error = None;
        self.detail_scroll = 0;
    }
}
