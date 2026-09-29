//! Browser navigation and selection.

use super::*;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Edge {
    Start,
    End,
}

impl App {
    pub(super) fn handle_browser_key(&mut self, code: KeyCode) -> Vec<QueryAction> {
        match code {
            KeyCode::Char('r') => {
                self.status = None;
                vec![self.thread_action(self.search_query.clone(), self.page_offset)]
            }
            KeyCode::Char('n') => self
                .next_offset
                .map(|offset| vec![self.thread_action(self.search_query.clone(), offset)])
                .unwrap_or_default(),
            KeyCode::Char('p') if self.page_offset > 0 => {
                let offset = self.page_offset.saturating_sub(u64::from(PAGE_SIZE));
                vec![self.thread_action(self.search_query.clone(), offset)]
            }
            KeyCode::Tab => {
                self.focus = self.next_focus();
                Vec::new()
            }
            KeyCode::BackTab => {
                self.focus = self.previous_focus();
                Vec::new()
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                Vec::new()
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                Vec::new()
            }
            KeyCode::PageUp => {
                self.move_page(-1);
                Vec::new()
            }
            KeyCode::PageDown => {
                self.move_page(1);
                Vec::new()
            }
            KeyCode::Home => {
                self.move_to_edge(Edge::Start);
                Vec::new()
            }
            KeyCode::End => {
                self.move_to_edge(Edge::End);
                Vec::new()
            }
            KeyCode::Enter => self.select(),
            _ => Vec::new(),
        }
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

    fn move_to_edge(&mut self, edge: Edge) {
        let end = edge == Edge::End;
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
}
