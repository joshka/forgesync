//! # Draw recorded acquisition failures
//!
//! [`draw_failures`] presents the app's loaded recent-run projection and unresolved-work text for
//! its selected run. Wide areas place list and detail side by side; compact areas stack them.
//! Query and app owners select runs and prepare entry strings before rendering; this module does
//! not filter the ledger, resolve targets, or infer source completeness from a run status.
//!
//! The list distinguishes loading without retained rows, a recorded load error, and an empty
//! projection. Selection highlighting is suppressed for errors and empty data. Detail prefers a
//! currently selected retained run, then a load error, then a selection hint, so a list error can
//! coexist with previously loaded detail until app state replaces that projection.
//!
//! A run with no entry strings gets an explicit absence message. The view does not invent retry
//! targets from that absence or claim that every family is complete. The underlying run/coverage
//! relationship remains an engine and store concern.
//!
//! Rendering borrows app state and uses temporary widget selection state. Retry and refresh are
//! separate input-driven workflows; drawing issues no archive reads, writes, or provider requests.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

use crate::app::App;
use crate::view::{COMPACT_WIDTH, pane_block, selected_style};

/// Arranges the failed-run list and selected failure detail.
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

/// Draws non-complete runs and the current selection.
fn draw_failure_list(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut items = app
        .failure_list
        .items
        .iter()
        .map(|run| ListItem::new(format!("Run #{} · {:?}", run.id, run.status)))
        .collect::<Vec<_>>();
    if app.failure_list.loading && items.is_empty() {
        items.push(ListItem::new("Loading recent runs…"));
    } else if let Some(error) = &app.failure_list.error {
        items = vec![ListItem::new(error.clone())];
    } else if items.is_empty() {
        items.push(ListItem::new("No incomplete or failed runs"));
    }
    let mut state = ListState::default();
    if app.failure_list.error.is_none() && !app.failure_list.items.is_empty() {
        state.select(Some(app.failure_list.selected));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(pane_block("Recent runs", true))
            .highlight_style(selected_style()),
        area,
        &mut state,
    );
}

/// Draws unresolved failure entries for the selected run.
fn draw_failure_detail(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let lines = if let Some(run) = app.failure_list.items.get(app.failure_list.selected) {
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
    } else if let Some(error) = &app.failure_list.error {
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
