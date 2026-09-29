//! # Handle archive browser navigation
//!
//! These `App` methods move through repository and thread lists, open details, change focus, and
//! request related coverage or failure data. They interpret keys according to the browser's
//! current selection.
//!
//! The methods update state or start a query; they do not draw widgets. Keeping browser input
//! beside its navigation semantics makes it easier to see what each key means on each screen.

use crossterm::event::KeyCode;
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};

use crate::app::{App, Focus, PAGE_SIZE, move_index};
use crate::query::QueryAction;

/// A navigation intent interpreted against the active pane's own bounds.
#[derive(Clone, Copy)]
enum Movement {
    Step(i8),
    Page(i8),
    Start,
    End,
}

impl Movement {
    /// Computes a bounded position; detail scrolling supplies its own maximum.
    fn position(self, current: usize, maximum: usize) -> usize {
        match self {
            Self::Step(direction) => move_index(current, maximum, direction),
            Self::Page(direction) if direction < 0 => current.saturating_sub(10),
            Self::Page(_) => current.saturating_add(10).min(maximum),
            Self::Start => 0,
            Self::End => maximum,
        }
    }
}

impl App {
    /// Maps browser keys to named actions without blocking on archive I/O.
    pub fn handle_browser_key(&mut self, code: KeyCode) -> Vec<QueryAction> {
        match code {
            KeyCode::Char('r') => self.reload_threads(),
            KeyCode::Char('n') => self.next_thread_page(),
            KeyCode::Char('p') => self.previous_thread_page(),
            KeyCode::Tab => self.advance_focus(),
            KeyCode::BackTab => self.retreat_focus(),
            KeyCode::Up | KeyCode::Char('k') => self.navigate(Movement::Step(-1)),
            KeyCode::Down | KeyCode::Char('j') => self.navigate(Movement::Step(1)),
            KeyCode::PageUp => self.navigate(Movement::Page(-1)),
            KeyCode::PageDown => self.navigate(Movement::Page(1)),
            KeyCode::Home => self.navigate(Movement::Start),
            KeyCode::End => self.navigate(Movement::End),
            KeyCode::Enter => self.select(),
            _ => Vec::new(),
        }
    }

    /// Clears a prior status before reloading the currently displayed query page.
    fn reload_threads(&mut self) -> Vec<QueryAction> {
        self.status = None;
        vec![self.thread_action(self.search_query.clone(), self.page_offset)]
    }

    /// Requests the server-provided next local page when one exists.
    fn next_thread_page(&mut self) -> Vec<QueryAction> {
        self.next_offset
            .map(|offset| vec![self.thread_action(self.search_query.clone(), offset)])
            .unwrap_or_default()
    }

    /// Requests the preceding page, keeping the first page stable.
    fn previous_thread_page(&mut self) -> Vec<QueryAction> {
        if self.page_offset == 0 {
            return Vec::new();
        }
        let offset = self.page_offset.saturating_sub(u64::from(PAGE_SIZE));
        vec![self.thread_action(self.search_query.clone(), offset)]
    }

    /// Advances focus without changing selection or requesting data.
    fn advance_focus(&mut self) -> Vec<QueryAction> {
        self.focus = match self.focus {
            Focus::Repositories => Focus::Threads,
            Focus::Threads => Focus::Detail,
            Focus::Detail => Focus::Repositories,
        };
        Vec::new()
    }

    /// Retreats focus without changing selection or requesting data.
    fn retreat_focus(&mut self) -> Vec<QueryAction> {
        self.focus = match self.focus {
            Focus::Repositories => Focus::Detail,
            Focus::Threads => Focus::Repositories,
            Focus::Detail => Focus::Threads,
        };
        Vec::new()
    }

    /// Applies navigation using the active pane's selection and invalidation contract.
    fn navigate(&mut self, movement: Movement) -> Vec<QueryAction> {
        match self.focus {
            Focus::Repositories => self.navigate_repositories(movement),
            Focus::Threads => self.navigate_threads(movement),
            Focus::Detail => self.navigate_detail(movement),
        }
        Vec::new()
    }

    /// Includes the synthetic all-repositories row in the picker bounds.
    fn navigate_repositories(&mut self, movement: Movement) {
        self.repository_cursor = movement.position(self.repository_cursor, self.repositories.len());
    }

    /// Invalidates loaded detail when a thread selection changes, even within one page.
    fn navigate_threads(&mut self, movement: Movement) {
        if self.threads.is_empty() {
            // Preserve Home/End scrolling when the thread list has no selection.
            if matches!(movement, Movement::Start | Movement::End) {
                self.navigate_detail(movement);
            }
            return;
        }
        let maximum = self.threads.len() - 1;
        let current = self.selected_thread.unwrap_or(0);
        self.selected_thread = Some(movement.position(current, maximum));
        self.invalidate_detail();
    }

    /// Saturates scrolling here; rendering clamps the requested position to visible content.
    fn navigate_detail(&mut self, movement: Movement) {
        let position = movement.position(usize::from(self.detail_scroll), usize::from(u16::MAX));
        self.detail_scroll = u16::try_from(position).expect("position is bounded by u16::MAX");
    }

    /// Opens the selected item according to the active browser pane.
    fn select(&mut self) -> Vec<QueryAction> {
        match self.focus {
            Focus::Repositories => self.select_repository(),
            Focus::Threads => self.select_thread(),
            Focus::Detail => Vec::new(),
        }
    }

    /// Applies a changed repository filter and resets its page and detail together.
    fn select_repository(&mut self) -> Vec<QueryAction> {
        let selected = self.repository_cursor.checked_sub(1);
        if selected == self.applied_repository {
            return Vec::new();
        }
        self.status = None;
        self.applied_repository = selected;
        self.page_offset = 0;
        self.detail = None;
        vec![self.thread_action(self.search_query.clone(), 0)]
    }

    /// Requests detail for an existing selection and transfers focus to its pane.
    fn select_thread(&mut self) -> Vec<QueryAction> {
        let Some(summary) = self
            .selected_thread
            .and_then(|index| self.threads.get(index))
        else {
            return Vec::new();
        };
        let repository = RepositorySelector::from_repository(&summary.repository);
        let selector = ThreadSelector::new(repository, summary.discussion.id.number());
        self.focus = Focus::Detail;
        vec![QueryAction::Detail(selector)]
    }
}
