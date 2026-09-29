//! Interpret keys as navigation and maintainer actions.

use super::*;

mod browser;
mod triage;

impl App {
    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> Vec<QueryAction> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return self.request_quit();
        }
        if self.searching {
            return self.handle_search_key(key.code);
        }
        if let Some(actions) = self.handle_global_key(key.code) {
            return actions;
        }
        if key.code == KeyCode::Esc {
            return self.leave_current_view();
        }
        match self.screen {
            Screen::Browser => self.handle_browser_key(key.code),
            Screen::Coverage => Vec::new(),
            Screen::Failures => self.handle_failures_key(key.code),
            Screen::Clusters => self.handle_clusters_key(key.code),
            Screen::ClusterDetail => self.handle_cluster_detail_key(key.code),
        }
    }

    fn handle_global_key(&mut self, code: KeyCode) -> Option<Vec<QueryAction>> {
        let actions = match code {
            KeyCode::Char('q') => self.request_quit(),
            KeyCode::Char('c') => {
                self.screen = Screen::Coverage;
                self.status = None;
                vec![QueryAction::Coverage]
            }
            KeyCode::Char('f') => {
                self.screen = Screen::Failures;
                self.status = None;
                vec![QueryAction::Failures]
            }
            KeyCode::Char('g') => {
                self.screen = Screen::Clusters;
                self.status = None;
                vec![QueryAction::Clusters {
                    repositories: self.repository_scope(),
                }]
            }
            KeyCode::Char('s') => vec![QueryAction::Sync {
                repositories: self.repository_scope(),
            }],
            KeyCode::Char('R') => vec![QueryAction::Refresh {
                repositories: self.repository_scope(),
            }],
            KeyCode::Char('/') => {
                self.screen = Screen::Browser;
                self.searching = true;
                self.search_input = self.search_query.clone().unwrap_or_default();
                Vec::new()
            }
            _ => return None,
        };
        Some(actions)
    }

    fn leave_current_view(&mut self) -> Vec<QueryAction> {
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
