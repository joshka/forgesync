//! # Draw repository and discussion browsing
//!
//! `draw_browser` arranges the current repository, thread list, and selected discussion detail in
//! terminal space. It uses `App` selections and loaded projections to show where the user is in
//! the archive.
//!
//! The engine constructs the underlying offline view; this module chooses layout, labels, and
//! focus cues. It should render loading and failure state from the app rather than silently
//! implying an empty archive.

use super::detail::detail_lines;
use super::{
    App, COMPACT_WIDTH, Constraint, Direction, Focus, Frame, Layout, List, ListItem, ListState,
    Paragraph, Rect, Text, Wrap, pane_block, selected_style,
};

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
    let title = if let Some(index) = app.repository_picker.applied {
        app.repository_picker
            .items
            .get(index)
            .map(|repository| format!("Repositories · {}", repository.full_name))
            .unwrap_or_else(|| "Repositories".to_owned())
    } else {
        "Repositories · all".to_owned()
    };
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
        .threads
        .iter()
        .map(|thread| {
            let number = thread.discussion.id.number().get();
            ListItem::new(format!("#{} {}", number, thread.discussion.title))
        })
        .collect();
    if app.threads_loading && app.threads.is_empty() {
        items.push(ListItem::new("Loading discussions…"));
    } else if let Some(error) = &app.thread_error {
        items = vec![ListItem::new(error.clone())];
    } else if app.threads.is_empty() {
        items.push(ListItem::new("No discussions"));
    }
    let title = format!(
        "Discussions · {}{}",
        app.threads.len(),
        if app.threads_loading {
            " · loading"
        } else {
            ""
        }
    );
    let mut state = ListState::default();
    if app.thread_error.is_none() && !app.threads.is_empty() {
        state.select(app.selected_thread);
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
    app.detail_scroll = app
        .detail_scroll
        .min(u16::try_from(max_scroll).unwrap_or(u16::MAX));
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0)),
        area,
    );
}
