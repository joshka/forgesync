//! Repository picker, discussion list, and selected discussion detail.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::Text;
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, Focus};
use crate::view::detail::detail_lines;
use crate::view::{COMPACT_WIDTH, pane, selected_style};

pub fn draw_browser(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let (direction, constraints) = if area.width >= COMPACT_WIDTH {
        (
            Direction::Horizontal,
            [
                Constraint::Percentage(25),
                Constraint::Percentage(35),
                Constraint::Percentage(40),
            ],
        )
    } else {
        (Direction::Vertical, [Constraint::Ratio(1, 3); 3])
    };
    let panes = Layout::default()
        .direction(direction)
        .constraints(constraints)
        .split(area);
    draw_repositories(frame, panes[0], app);
    draw_threads(frame, panes[1], app);
    draw_detail(frame, panes[2], app);
}

/// The title names the applied scope, which can differ from the highlighted row.
fn draw_repositories(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let picker = &mut app.repository_picker;
    let mut items = vec![ListItem::new("All repositories")];
    items.extend(
        picker
            .rows
            .data
            .iter()
            .map(|repository| ListItem::new(repository.full_name.clone())),
    );
    let mut placeholder = ListState::default();
    let mut state = &mut picker.state;
    if picker.rows.loading && picker.rows.data.is_empty() {
        items = vec![ListItem::new("Loading repositories…")];
    } else if let Some(error) = &picker.rows.error {
        items = vec![ListItem::new(error.clone())];
        state = &mut placeholder;
    }
    let title = picker
        .applied
        .as_ref()
        .map(|repository| format!("Repositories · {}", repository.full_name))
        .unwrap_or_else(|| "Repositories · all".to_owned());
    let list = List::new(items)
        .block(pane(&title, app.focus == Focus::Repositories))
        .highlight_style(selected_style());
    frame.render_stateful_widget(list, area, state);
}

fn draw_threads(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let threads = &mut app.thread_list;
    let rows = &threads.rows;
    let mut placeholder = ListState::default();
    let (items, state) = if rows.loading && rows.data.is_empty() {
        (
            vec![ListItem::new("Loading discussions…")],
            &mut placeholder,
        )
    } else if let Some(error) = &rows.error {
        (vec![ListItem::new(error.clone())], &mut placeholder)
    } else if rows.data.is_empty() {
        (vec![ListItem::new("No discussions")], &mut placeholder)
    } else {
        let items = rows
            .data
            .iter()
            .map(|thread| {
                let number = thread.discussion.id.number().get();
                ListItem::new(format!("#{} {}", number, thread.discussion.title))
            })
            .collect();
        (items, &mut threads.state)
    };
    let loading = if rows.loading { " · loading" } else { "" };
    let title = format!("Discussions · {}{loading}", rows.data.len());
    let list = List::new(items)
        .block(pane(&title, app.focus == Focus::Threads))
        .highlight_style(selected_style());
    frame.render_stateful_widget(list, area, state);
}

/// Clamps the stored scroll to the unwrapped line count, so wrapped lines can still overflow.
fn draw_detail(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let lines = detail_lines(app);
    let block = pane("Discussion detail", app.focus == Focus::Detail);
    let max_scroll = lines
        .len()
        .saturating_sub(usize::from(block.inner(area).height));
    let scroll = &mut app.detail_pane.scroll;
    *scroll = (*scroll).min(u16::try_from(max_scroll).unwrap_or(u16::MAX));
    let paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((*scroll, 0));
    frame.render_widget(paragraph, area);
}
