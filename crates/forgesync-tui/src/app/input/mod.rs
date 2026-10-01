//! Key routing.
//!
//! Precedence: Control-C, the search draft while editing, global keys, Escape, then the active
//! screen. Handlers change local state synchronously and return actions; they never await I/O.
//! Repository-scoped actions use the applied repository, not the highlighted picker row.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use forgesync_engine::reference::RepositorySelector;

use crate::app::{App, Focus, Screen};
use crate::query::{Operation, QueryAction, Read};

mod browser;
mod triage;

impl App {
    /// Routes a key by the precedence above and returns the work it requests.
    pub fn handle_key(&mut self, key: KeyEvent) -> Vec<QueryAction> {
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

    /// Handles keys that mean the same on every screen; `None` for screen-specific keys.
    fn handle_global_key(&mut self, code: KeyCode) -> Option<Vec<QueryAction>> {
        let actions = match code {
            KeyCode::Char('q') => self.request_quit(),
            KeyCode::Char('c') => self.show_coverage(),
            KeyCode::Char('f') => self.show_failures(),
            KeyCode::Char('g') => self.show_clusters(),
            KeyCode::Char('s') => self.sync_scope(),
            KeyCode::Char('R') => self.refresh_scope(),
            KeyCode::Char('/') => self.edit_search(),
            _ => return None,
        };
        Some(actions)
    }

    /// Shows coverage and reloads it, clearing the status line.
    fn show_coverage(&mut self) -> Vec<QueryAction> {
        self.screen = Screen::Coverage;
        self.status = None;
        vec![QueryAction::Read(Read::Coverage)]
    }

    /// Shows recent failed runs and reloads them, clearing the status line.
    fn show_failures(&mut self) -> Vec<QueryAction> {
        self.screen = Screen::Failures;
        self.status = None;
        vec![QueryAction::Read(Read::Failures)]
    }

    /// Shows clusters for the applied scope and reloads them, clearing the status line.
    fn show_clusters(&mut self) -> Vec<QueryAction> {
        self.screen = Screen::Clusters;
        self.status = None;
        vec![QueryAction::Read(Read::Clusters {
            repositories: self.repository_scope(),
        })]
    }

    /// Syncs the applied repository, or every registered repository when none is applied.
    fn sync_scope(&self) -> Vec<QueryAction> {
        vec![QueryAction::Operation(Operation::Sync {
            repositories: self.repository_scope(),
        })]
    }

    /// Refreshes the applied repository, or every registered repository when none is applied.
    fn refresh_scope(&self) -> Vec<QueryAction> {
        vec![QueryAction::Operation(Operation::Refresh {
            repositories: self.repository_scope(),
        })]
    }

    /// Opens the search draft on the browser, prefilled with the applied query.
    fn edit_search(&mut self) -> Vec<QueryAction> {
        self.screen = Screen::Browser;
        self.searching = true;
        self.search_input = self.search_query.clone().unwrap_or_default();
        Vec::new()
    }

    /// Escape: cluster detail returns to the list and other screens to the browser; on the
    /// browser it clears an applied search, then moves focus from detail back to the list.
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

    /// Edits the search draft; results change only when it is submitted.
    fn handle_search_key(&mut self, code: KeyCode) -> Vec<QueryAction> {
        match code {
            KeyCode::Esc => self.cancel_search(),
            KeyCode::Enter => self.submit_search(),
            KeyCode::Backspace => self.erase_search_character(),
            KeyCode::Char(character) => self.append_search_character(character),
            _ => Vec::new(),
        }
    }

    /// Discards the draft and keeps the applied query.
    fn cancel_search(&mut self) -> Vec<QueryAction> {
        self.searching = false;
        self.search_input.clear();
        Vec::new()
    }

    /// Applies the trimmed draft; empty text clears the filter.
    fn submit_search(&mut self) -> Vec<QueryAction> {
        self.searching = false;
        let query = self.search_input.trim().to_owned();
        self.search_query = (!query.is_empty()).then_some(query);
        self.search_input.clear();
        vec![self.thread_action(self.search_query.clone(), 0)]
    }

    fn erase_search_character(&mut self) -> Vec<QueryAction> {
        self.search_input.pop();
        Vec::new()
    }

    fn append_search_character(&mut self, character: char) -> Vec<QueryAction> {
        self.search_input.push(character);
        Vec::new()
    }

    /// A thread read scoped to the applied repository.
    pub fn thread_action(&self, query: Option<String>, offset: u64) -> QueryAction {
        let repositories = self.repository_scope();
        QueryAction::Read(Read::Threads {
            query,
            repositories,
            offset,
        })
    }

    /// The applied repository as a selector list; empty selects every repository.
    pub fn repository_scope(&self) -> Vec<RepositorySelector> {
        self.repository_picker
            .applied
            .as_ref()
            .map(RepositorySelector::from_repository)
            .into_iter()
            .collect()
    }

    /// Quitting with a running writer requests cancellation and waits for its result instead.
    fn request_quit(&mut self) -> Vec<QueryAction> {
        if let Some(operation) = &mut self.operation {
            operation.cancelling = true;
            vec![QueryAction::CancelOperation]
        } else {
            self.quit = true;
            Vec::new()
        }
    }
}
