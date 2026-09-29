//! Failures screen rendering.

use super::{
    App, COMPACT_WIDTH, Color, Constraint, Direction, Frame, Layout, Line, List, ListItem,
    ListState, Modifier, Paragraph, Rect, Style, Text, Wrap, pane_block, selected_style,
};

pub fn draw_failures(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.width >= COMPACT_WIDTH {
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(area);
        draw_failure_list(frame, panes[0], app);
        draw_failure_detail(frame, panes[1], app);
    } else {
        let panes = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Ratio(1, 3), Constraint::Ratio(2, 3)])
            .split(area);
        draw_failure_list(frame, panes[0], app);
        draw_failure_detail(frame, panes[1], app);
    }
}

fn draw_failure_list(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut items = app
        .failures
        .iter()
        .map(|run| ListItem::new(format!("Run #{} · {:?}", run.id, run.status)))
        .collect::<Vec<_>>();
    if app.failures_loading && items.is_empty() {
        items.push(ListItem::new("Loading recent runs…"));
    } else if let Some(error) = &app.failures_error {
        items = vec![ListItem::new(error.clone())];
    } else if items.is_empty() {
        items.push(ListItem::new("No incomplete or failed runs"));
    }
    let mut state = ListState::default();
    if app.failures_error.is_none() && !app.failures.is_empty() {
        state.select(Some(app.selected_failure));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(pane_block("Recent runs", true))
            .highlight_style(selected_style()),
        area,
        &mut state,
    );
}

fn draw_failure_detail(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let lines = if let Some(run) = app.failures.get(app.selected_failure) {
        let mut lines = vec![
            Line::from(format!("Run #{} · {:?}", run.id, run.status)).style(
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
    } else if let Some(error) = &app.failures_error {
        vec![Line::from(error.clone())]
    } else {
        vec![Line::from(
            "Select an incomplete run to inspect or retry its unresolved work.",
        )]
    };
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(pane_block("Unresolved work", true))
            .wrap(Wrap { trim: false }),
        area,
    );
}
