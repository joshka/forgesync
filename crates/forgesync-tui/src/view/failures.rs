//! Recent unfinished runs and the selected run's unresolved work.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

use crate::app::App;
use crate::view::{COMPACT_WIDTH, pane, selected_style};

pub fn draw_failures(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let (direction, constraints) = if area.width >= COMPACT_WIDTH {
        (
            Direction::Horizontal,
            [Constraint::Percentage(35), Constraint::Percentage(65)],
        )
    } else {
        (
            Direction::Vertical,
            [Constraint::Ratio(1, 3), Constraint::Ratio(2, 3)],
        )
    };
    let panes = Layout::default()
        .direction(direction)
        .constraints(constraints)
        .split(area);
    draw_failure_list(frame, panes[0], app);
    draw_failure_detail(frame, panes[1], app);
}

/// Placeholder rows (loading, error, empty) are drawn without a highlight.
fn draw_failure_list(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let list = &mut app.failure_list;
    let rows = &list.rows;
    let mut placeholder = ListState::default();
    let (items, state) = if rows.loading && rows.data.is_empty() {
        (
            vec![ListItem::new("Loading recent runs…")],
            &mut placeholder,
        )
    } else if let Some(error) = &rows.error {
        (vec![ListItem::new(error.clone())], &mut placeholder)
    } else if rows.data.is_empty() {
        let empty = "No incomplete or failed runs";
        (vec![ListItem::new(empty)], &mut placeholder)
    } else {
        let items = rows
            .data
            .iter()
            .map(|run| ListItem::new(format!("Run #{} · {:?}", run.id.get(), run.status)))
            .collect();
        (items, &mut list.state)
    };
    let list = List::new(items)
        .block(pane("Recent runs", true))
        .highlight_style(selected_style());
    frame.render_stateful_widget(list, area, state);
}

/// A selected retained run is shown even while a refresh error is listed beside it.
fn draw_failure_detail(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let lines = if let Some(run) = app.failure_list.selected() {
        let heading = format!("Run #{} · {:?}", run.id.get(), run.status);
        let mut lines = vec![
            Line::from(heading).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::from(""),
        ];
        if run.entries.is_empty() {
            lines.push(Line::from("No unresolved failure details recorded."));
        } else {
            lines.extend(run.entries.iter().map(|entry| Line::from(entry.clone())));
        }
        lines
    } else if let Some(error) = &app.failure_list.rows.error {
        vec![Line::from(error.clone())]
    } else {
        vec![Line::from(
            "Select an incomplete run to inspect or retry its unresolved work.",
        )]
    };
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(pane("Unresolved work", true))
            .wrap(Wrap { trim: false }),
        area,
    );
}
