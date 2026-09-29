//! # Route key events to the active screen
//!
//! The input dispatcher reads the current `App` screen and focus, then sends a key to the relevant
//! browser or triage handler. Shared navigation and exit behavior stays at this level.
//!
//! `browser` owns thread navigation; `triage` owns cluster and decision interactions. A handler
//! may request asynchronous work through app/query state, but drawing remains in `view`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use forgesync_engine::reference::RepositorySelector;

use crate::app::{App, Focus, Screen};
use crate::query::requests::QueryAction;

mod browser;
mod triage;

impl App {
    /// Routes a key to the active screen while honoring global exit and search controls.
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

    /// Handles keys that have the same meaning on every screen.
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

    /// Requests coverage and clears unrelated status before displaying its screen.
    fn show_coverage(&mut self) -> Vec<QueryAction> {
        self.screen = Screen::Coverage;
        self.status = None;
        vec![QueryAction::Coverage]
    }

    /// Requests the durable failure ledger for the failure screen.
    fn show_failures(&mut self) -> Vec<QueryAction> {
        self.screen = Screen::Failures;
        self.status = None;
        vec![QueryAction::Failures]
    }

    /// Requests clusters within the applied browser scope.
    fn show_clusters(&mut self) -> Vec<QueryAction> {
        self.screen = Screen::Clusters;
        self.status = None;
        vec![QueryAction::Clusters {
            repositories: self.repository_scope(),
        }]
    }

    /// Starts acquisition using the applied repository filter rather than the picker highlight.
    fn sync_scope(&self) -> Vec<QueryAction> {
        vec![QueryAction::Sync {
            repositories: self.repository_scope(),
        }]
    }

    /// Starts the composed refresh workflow using the same scope as browsing.
    fn refresh_scope(&self) -> Vec<QueryAction> {
        vec![QueryAction::Refresh {
            repositories: self.repository_scope(),
        }]
    }

    /// Opens a local query draft; editing does not request archive or provider work.
    fn edit_search(&mut self) -> Vec<QueryAction> {
        self.screen = Screen::Browser;
        self.searching = true;
        self.search_input = self.search_query.clone().unwrap_or_default();
        Vec::new()
    }

    /// Returns to the browser or closes a transient view without losing its scope.
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

    /// Edits the pending search query until submission or cancellation.
    fn handle_search_key(&mut self, code: KeyCode) -> Vec<QueryAction> {
        match code {
            KeyCode::Esc => self.cancel_search(),
            KeyCode::Enter => self.submit_search(),
            KeyCode::Backspace => self.erase_search_character(),
            KeyCode::Char(character) => self.append_search_character(character),
            _ => Vec::new(),
        }
    }

    /// Discards the draft while retaining the applied search query and page.
    fn cancel_search(&mut self) -> Vec<QueryAction> {
        self.searching = false;
        self.search_input.clear();
        Vec::new()
    }

    /// Applies a trimmed draft and requests the first page, with empty text clearing the filter.
    fn submit_search(&mut self) -> Vec<QueryAction> {
        self.searching = false;
        let query = self.search_input.trim().to_owned();
        self.search_query = (!query.is_empty()).then_some(query);
        self.search_input.clear();
        vec![self.thread_action(self.search_query.clone(), 0)]
    }

    /// Removes one Unicode scalar from the pending draft without changing results.
    fn erase_search_character(&mut self) -> Vec<QueryAction> {
        self.search_input.pop();
        Vec::new()
    }

    /// Extends the pending draft without requesting results until submission.
    fn append_search_character(&mut self, character: char) -> Vec<QueryAction> {
        self.search_input.push(character);
        Vec::new()
    }

    /// Builds a thread query from the applied repository filter. A highlighted but unapplied
    /// repository does not change the query scope.
    pub fn thread_action(&self, query: Option<String>, offset: u64) -> QueryAction {
        let repositories = self.repository_scope();
        QueryAction::Threads {
            query,
            repositories,
            offset,
        }
    }

    /// Returns the applied repository filter for actions that need the same scope as browsing.
    pub fn repository_scope(&self) -> Vec<RepositorySelector> {
        self.repository_picker
            .applied
            .as_ref()
            .map(RepositorySelector::from_repository)
            .into_iter()
            .collect()
    }

    /// Cancels an active writer before allowing the terminal to close.
    fn request_quit(&mut self) -> Vec<QueryAction> {
        if self.operation.busy() {
            self.status = Some("Cancellation requested; waiting for the active action…".to_owned());
            vec![QueryAction::CancelOperation]
        } else {
            self.quit = true;
            Vec::new()
        }
    }
}
