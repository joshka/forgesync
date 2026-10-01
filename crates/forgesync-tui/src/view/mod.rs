//! Drawing. Renderers format already loaded app state and never start queries; the mutable borrow
//! is only for list scroll state and clamping the detail scroll.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, Screen};

mod browser;
mod clusters;
mod coverage;
mod detail;
mod failures;
mod timeline;

use browser::draw_browser;
use clusters::{draw_cluster_detail, draw_clusters};
use coverage::draw_coverage;
use failures::draw_failures;

/// Switches panes to a vertical layout below the width needed for two readable columns.
const COMPACT_WIDTH: u16 = 100;

/// Selects the active screen renderer and keeps header and footer visible.
pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(frame.area());
    draw_header(frame, sections[0], app);
    match app.screen {
        Screen::Browser => draw_browser(frame, sections[1], app),
        Screen::Coverage => draw_coverage(frame, sections[1], app),
        Screen::Failures => draw_failures(frame, sections[1], app),
        Screen::Clusters => draw_clusters(frame, sections[1], app),
        Screen::ClusterDetail => draw_cluster_detail(frame, sections[1], app),
    }
    draw_footer(frame, sections[2], app);
}

/// Shows the active screen and submitted search scope.
fn draw_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let view = match app.screen {
        Screen::Browser => "Browse",
        Screen::Coverage => "Coverage",
        Screen::Failures => "Failures",
        Screen::Clusters => "Clusters",
        Screen::ClusterDetail => "Cluster members",
    };
    let query = app
        .search_query
        .as_ref()
        .map(|query| format!(" · /{query}"))
        .unwrap_or_default();
    let line = Line::from(format!(" Forgesync · {view}{query} ")).style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(Paragraph::new(line), area);
}

/// Shows operation progress, status, or key hints according to current state.
fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let text = if app.searching {
        format!(" Search: {}▏  Enter search · Esc cancel", app.search_input)
    } else if let Some(operation) = &app.operation {
        let label = operation.label;
        let cancel_hint = if operation.cancelling {
            "Cancellation requested; waiting for the active action…"
        } else {
            "q cancel"
        };
        if let Some(progress) = &operation.progress {
            format!(
                " {label} · {:?} · {}/{} jobs · {} threads · {} comments · {} · {cancel_hint}",
                progress.status,
                progress.completed_jobs,
                progress.total_jobs,
                progress.threads_seen,
                progress.comments_seen,
                progress.repository.as_deref().unwrap_or("all repositories")
            )
        } else {
            format!(" {label} starting… · {cancel_hint}")
        }
    } else if let Some(status) = &app.status {
        format!(" {status}  ·  q quit")
    } else {
        match app.screen {
            Screen::Browser => " Tab focus · Enter select · / search · g clusters · s sync · R refresh · c coverage · f failures · q quit".to_owned(),
            Screen::Coverage => " c reload · g clusters · s sync · R refresh · f failures · Esc back · q quit".to_owned(),
            Screen::Failures => " ↑/↓ select · t retry selected run · g clusters · Esc back · q quit".to_owned(),
            Screen::Clusters => " ↑/↓ select · Enter members · d dismiss/restore · g reload · Esc back · q quit".to_owned(),
            Screen::ClusterDetail => " ↑/↓ select · e exclude · i include · k canonical · d dismiss/restore · Esc clusters · q quit".to_owned(),
        }
    };
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::Gray)),
        area,
    );
}

/// A titled pane border, cyan when emphasized (focused) and dark gray otherwise.
fn pane(title: &str, emphasized: bool) -> Block<'_> {
    let color = if emphasized {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(color))
}

fn selected_style() -> Style {
    Style::default()
        .fg(Color::White)
        .bg(Color::Blue)
        .add_modifier(Modifier::BOLD)
}

// Core's `EvidenceFamily` has no `as_str`; the CLI keeps its own copy of these labels.
fn family_name(family: forgesync_core::coverage::EvidenceFamily) -> &'static str {
    match family {
        forgesync_core::coverage::EvidenceFamily::Threads => "threads",
        forgesync_core::coverage::EvidenceFamily::Comments => "comments",
        forgesync_core::coverage::EvidenceFamily::PullRequestMetadata => "pull request metadata",
        forgesync_core::coverage::EvidenceFamily::Reviews => "reviews",
        forgesync_core::coverage::EvidenceFamily::ReviewThreads => "review threads",
    }
}

#[cfg(test)]
mod tests;
