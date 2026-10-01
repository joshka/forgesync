//! Browser keys: focus, list movement, paging, and opening the focused row.

use crossterm::event::KeyCode;
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};
use ratatui::widgets::ListState;

use crate::app::{App, Focus, PAGE_SIZE};
use crate::query::{QueryAction, Read};

impl App {
    pub fn handle_browser_key(&mut self, code: KeyCode) -> Vec<QueryAction> {
        match code {
            KeyCode::Char('r') => self.reload_threads(),
            KeyCode::Char('n') => self.next_thread_page(),
            KeyCode::Char('p') => self.previous_thread_page(),
            KeyCode::Tab => self.advance_focus(),
            KeyCode::BackTab => self.retreat_focus(),
            KeyCode::Up | KeyCode::Char('k') => self
                .navigate(ListState::select_previous, |scroll| {
                    scroll.saturating_sub(1)
                }),
            KeyCode::Down | KeyCode::Char('j') => {
                self.navigate(ListState::select_next, |scroll| scroll.saturating_add(1))
            }
            KeyCode::PageUp => self.navigate(
                |state| state.scroll_up_by(10),
                |scroll| scroll.saturating_sub(10),
            ),
            KeyCode::PageDown => self.navigate(
                |state| state.scroll_down_by(10),
                |scroll| scroll.saturating_add(10),
            ),
            KeyCode::Home => self.navigate(ListState::select_first, |_| 0),
            KeyCode::End => self.navigate(ListState::select_last, |_| u16::MAX),
            KeyCode::Enter => self.select(),
            _ => Vec::new(),
        }
    }

    /// Reloads the current page and clears the status line.
    fn reload_threads(&mut self) -> Vec<QueryAction> {
        self.status = None;
        vec![self.thread_action(self.search_query.clone(), self.thread_list.offset)]
    }

    /// Requests the next page reported by the store, if any.
    fn next_thread_page(&mut self) -> Vec<QueryAction> {
        self.thread_list
            .next_offset
            .map(|offset| vec![self.thread_action(self.search_query.clone(), offset)])
            .unwrap_or_default()
    }

    /// Requests the preceding page; does nothing on the first page.
    fn previous_thread_page(&mut self) -> Vec<QueryAction> {
        if self.thread_list.offset == 0 {
            return Vec::new();
        }
        let offset = self.thread_list.offset.saturating_sub(u64::from(PAGE_SIZE));
        vec![self.thread_action(self.search_query.clone(), offset)]
    }

    fn advance_focus(&mut self) -> Vec<QueryAction> {
        self.focus = match self.focus {
            Focus::Repositories => Focus::Threads,
            Focus::Threads => Focus::Detail,
            Focus::Detail => Focus::Repositories,
        };
        Vec::new()
    }

    fn retreat_focus(&mut self) -> Vec<QueryAction> {
        self.focus = match self.focus {
            Focus::Repositories => Focus::Detail,
            Focus::Threads => Focus::Repositories,
            Focus::Detail => Focus::Threads,
        };
        Vec::new()
    }

    /// Moves the focused list or scrolls detail; rendering clamps the scroll to the content.
    fn navigate(&mut self, step: fn(&mut ListState), scroll: fn(u16) -> u16) -> Vec<QueryAction> {
        match self.focus {
            Focus::Repositories => self.repository_picker.select(step),
            Focus::Threads => {
                if !self.thread_list.rows.data.is_empty() {
                    self.thread_list.select(step);
                    self.detail_pane.invalidate();
                }
            }
            Focus::Detail => self.detail_pane.scroll = scroll(self.detail_pane.scroll),
        }
        Vec::new()
    }

    /// Enter: applies the highlighted repository or opens the selected discussion.
    fn select(&mut self) -> Vec<QueryAction> {
        match self.focus {
            Focus::Repositories => self.select_repository(),
            Focus::Threads => self.select_thread(),
            Focus::Detail => Vec::new(),
        }
    }

    /// Applies the highlighted repository as the browsing scope and reloads the first page.
    fn select_repository(&mut self) -> Vec<QueryAction> {
        let selected = self.repository_picker.highlighted().cloned();
        if selected == self.repository_picker.applied {
            return Vec::new();
        }
        self.status = None;
        self.repository_picker.applied = selected;
        self.thread_list.offset = 0;
        self.detail_pane.invalidate();
        vec![self.thread_action(self.search_query.clone(), 0)]
    }

    /// Requests detail for the selected discussion and focuses the detail pane.
    fn select_thread(&mut self) -> Vec<QueryAction> {
        let Some(summary) = self.thread_list.selected() else {
            return Vec::new();
        };
        let repository = RepositorySelector::from_repository(&summary.repository);
        let selector = ThreadSelector::new(repository, summary.discussion.id.number());
        self.focus = Focus::Detail;
        vec![Read::Detail(selector).into()]
    }
}
