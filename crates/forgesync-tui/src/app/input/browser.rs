//! # Handle archive browser navigation
//!
//! These `App` methods move through repository and thread lists, open details, change focus, and
//! request thread pages and selected details. They interpret keys according to the browser's
//! current selection.
//!
//! The methods update state or start a query; they do not draw widgets. Keeping browser input
//! beside its navigation semantics makes it easier to see what each key means on each screen.
//!
//! `Movement` expresses a step, ten-position page, or endpoint movement. Each focused pane applies
//! that intent to its own bounds: repository highlight, thread selection, or detail scroll. Moving
//! a list does not fetch a new page; explicit next/previous-page keys use local page coordinates.
//!
//! Enter interprets focus rather than merely opening the highlighted row. Applying a repository
//! changes the stable browser scope and requests its first page; selecting a thread begins a
//! generation-tagged detail request. Navigation invalidates stale detail through the owning pane
//! state instead of leaving another thread's content visible as the current selection.
//!
//! These methods produce actions but perform no archive or network I/O. The event loop dispatches
//! them, query owners retrieve data, and reply handlers decide whether the result is still current.

use crossterm::event::KeyCode;
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};

use crate::app::{App, Focus, PAGE_SIZE, move_index};
use crate::query::requests::QueryAction;

/// A navigation intent interpreted against the active pane's own bounds.
#[derive(Clone, Copy)]
enum Movement {
    /// Signed one-position movement, bounded by the focused pane.
    Step(i8),
    /// Signed ten-position movement within the loaded pane.
    Page(i8),
    /// First position of the focused pane.
    Start,
    /// Last available position of the focused pane.
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
        vec![self.thread_action(self.search_query.clone(), self.thread_list.offset)]
    }

    /// Requests the server-provided next local page when one exists.
    fn next_thread_page(&mut self) -> Vec<QueryAction> {
        self.thread_list
            .next_offset
            .map(|offset| vec![self.thread_action(self.search_query.clone(), offset)])
            .unwrap_or_default()
    }

    /// Requests the preceding page, keeping the first page stable.
    fn previous_thread_page(&mut self) -> Vec<QueryAction> {
        if self.thread_list.offset == 0 {
            return Vec::new();
        }
        let offset = self.thread_list.offset.saturating_sub(u64::from(PAGE_SIZE));
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
        self.repository_picker.cursor = movement.position(
            self.repository_picker.cursor,
            self.repository_picker.items.len(),
        );
    }

    /// Invalidates loaded detail when a thread selection changes, even within one page.
    fn navigate_threads(&mut self, movement: Movement) {
        if self.thread_list.items.is_empty() {
            // Preserve Home/End scrolling when the thread list has no selection.
            if matches!(movement, Movement::Start | Movement::End) {
                self.navigate_detail(movement);
            }
            return;
        }
        let maximum = self.thread_list.items.len() - 1;
        let current = self.thread_list.selected.unwrap_or(0);
        self.thread_list.selected = Some(movement.position(current, maximum));
        self.detail_pane.invalidate();
    }

    /// Saturates scrolling here; rendering clamps the requested position to visible content.
    fn navigate_detail(&mut self, movement: Movement) {
        let position =
            movement.position(usize::from(self.detail_pane.scroll), usize::from(u16::MAX));
        self.detail_pane.scroll = u16::try_from(position).expect("position is bounded by u16::MAX");
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
        let selected = self
            .repository_picker
            .cursor
            .checked_sub(1)
            .and_then(|index| self.repository_picker.items.get(index))
            .cloned();
        if selected == self.repository_picker.applied {
            return Vec::new();
        }
        self.status = None;
        self.repository_picker.applied = selected;
        self.thread_list.offset = 0;
        self.detail_pane.invalidate();
        vec![self.thread_action(self.search_query.clone(), 0)]
    }

    /// Requests detail for an existing selection and transfers focus to its pane.
    fn select_thread(&mut self) -> Vec<QueryAction> {
        let Some(summary) = self
            .thread_list
            .selected
            .and_then(|index| self.thread_list.items.get(index))
        else {
            return Vec::new();
        };
        let repository = RepositorySelector::from_repository(&summary.repository);
        let selector = ThreadSelector::new(repository, summary.discussion.id.number());
        self.focus = Focus::Detail;
        vec![QueryAction::Detail(selector)]
    }
}
