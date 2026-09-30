//! # Draw repository and discussion browsing
//!
//! [`draw_browser`] arranges the repository picker, discussion page, and selected discussion detail
//! using projections already loaded into [`App`]. Wide areas use three columns; compact areas use
//! three stacked panes. This module owns layout, labels, selection highlighting, and focus cues,
//! while input handlers and asynchronous query owners decide which projections to load.
//!
//! The repository pane distinguishes the picker cursor from the applied repository filter: the
//! highlighted row can be a pending choice while its title describes the active scope. Discussion
//! rows use the loaded page and its selection rather than querying the archive during drawing.
//! Loading placeholders and errors come from app state; retained rows can remain visible during a
//! refresh, and an error replaces list rows when the loading-placeholder condition does not apply.
//!
//! Detail text is assembled by the sibling detail module, including its loading and failure states.
//! Drawing clamps the stored detail scroll to a bound derived from the current lines and pane
//! height. This is a presentation-state mutation, not a durable archive write or navigation action;
//! the line-based bound does not measure the extra terminal rows introduced by wrapping.
//!
//! Per-frame list widget state is temporary. No rendering helper performs provider I/O, archive
//! reads, or acquisition, and displaying retained content does not certify current source coverage.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::Text;
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, Focus};
use crate::view::detail::detail_lines;
use crate::view::{COMPACT_WIDTH, pane_block, selected_style};

/// Arranges repository, discussion, and detail panes for the available width.
pub fn draw_browser(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    if area.width >= COMPACT_WIDTH {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(35),
                Constraint::Percentage(40),
            ])
            .split(area);
        draw_repositories(frame, columns[0], app);
        draw_threads(frame, columns[1], app);
        draw_detail(frame, columns[2], app);
    } else {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
            ])
            .split(area);
        draw_repositories(frame, rows[0], app);
        draw_threads(frame, rows[1], app);
        draw_detail(frame, rows[2], app);
    }
}

/// Draws registered repositories with the active picker selection.
fn draw_repositories(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut items = vec![ListItem::new("All repositories")];
    items.extend(
        app.repository_picker
            .items
            .iter()
            .map(|repository| ListItem::new(repository.full_name.clone())),
    );
    if app.repository_picker.loading && app.repository_picker.items.is_empty() {
        items = vec![ListItem::new("Loading repositories…")];
    } else if let Some(error) = &app.repository_picker.error {
        items = vec![ListItem::new(error.clone())];
    }
    let title = app
        .repository_picker
        .applied
        .as_ref()
        .map(|repository| format!("Repositories · {}", repository.full_name))
        .unwrap_or_else(|| "Repositories · all".to_owned());
    let mut state = ListState::default();
    if app.repository_picker.error.is_none() {
        state.select(Some(
            app.repository_picker
                .cursor
                .min(items.len().saturating_sub(1)),
        ));
    }
    let block = pane_block(&title, app.focus == Focus::Repositories);
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(selected_style()),
        area,
        &mut state,
    );
}

/// Draws the current discussion page and selected row.
fn draw_threads(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut items: Vec<ListItem<'_>> = app
        .thread_list
        .items
        .iter()
        .map(|thread| {
            let number = thread.discussion.id.number().get();
            ListItem::new(format!("#{} {}", number, thread.discussion.title))
        })
        .collect();
    if app.thread_list.loading && app.thread_list.items.is_empty() {
        items.push(ListItem::new("Loading discussions…"));
    } else if let Some(error) = &app.thread_list.error {
        items = vec![ListItem::new(error.clone())];
    } else if app.thread_list.items.is_empty() {
        items.push(ListItem::new("No discussions"));
    }
    let title = format!(
        "Discussions · {}{}",
        app.thread_list.items.len(),
        if app.thread_list.loading {
            " · loading"
        } else {
            ""
        }
    );
    let mut state = ListState::default();
    if app.thread_list.error.is_none() && !app.thread_list.items.is_empty() {
        state.select(app.thread_list.selected);
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(pane_block(&title, app.focus == Focus::Threads))
            .highlight_style(selected_style()),
        area,
        &mut state,
    );
}

/// Draws selected discussion content or its loading and failure state.
fn draw_detail(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let lines = detail_lines(app);
    let block = pane_block("Discussion detail", app.focus == Focus::Detail);
    let inner_height = block.inner(area).height;
    let max_scroll = lines.len().saturating_sub(usize::from(inner_height));
    app.detail_pane.scroll = app
        .detail_pane
        .scroll
        .min(u16::try_from(max_scroll).unwrap_or(u16::MAX));
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((app.detail_pane.scroll, 0)),
        area,
    );
}
