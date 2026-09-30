//! # Draw the current app screen
//!
//! The top-level `draw` function chooses a screen renderer from `App` state. `browser` draws
//! repository and discussion navigation, `coverage` and `failures` expose evidence and work
//! status, and `clusters` draws duplicate triage views.
//!
//! View functions format already loaded data. They should not start queries or change durable
//! decisions. This keeps a frame deterministic for a given app state and lets input and data
//! loading remain independently understandable.
//!
//! [`draw`] reserves header and footer rows around the active screen body. Browser and cluster
//! renderers choose compact or split layouts based on available width; specialized leaves project
//! loaded detail, timeline, coverage, and failure values into readable sections. Shared helpers
//! provide panel borders and selection styles rather than owning acquisition or navigation policy.
//!
//! The mutable app borrow supports Ratatui's widget selection/render state. Rendering may update
//! that presentation state, but does not apply repository scope, initiate queries, or persist local
//! decisions. Pending, empty, failed, and ready data are interpreted from the app's existing state.
//! The footer reflects available controls and current status; it does not authorize an operation.
//!
//! The event loop owns frame timing and terminal lifecycle. Input produces actions, query workers
//! acquire results, and app reply handlers update loaded state before another frame is drawn.

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
    } else if app.operation.busy() {
        let label = app.operation.label().unwrap_or("action");
        let cancel_hint = app
            .status
            .as_deref()
            .filter(|status| status.starts_with("Cancellation requested"))
            .unwrap_or("q cancel");
        if let Some(progress) = app.operation.progress() {
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

/// Creates the shared bordered panel style for terminal panes.
fn pane_block(title: &str, focused: bool) -> Block<'_> {
    let color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(color))
}

/// Returns the highlight style used for the active row.
fn selected_style() -> Style {
    Style::default()
        .fg(Color::White)
        .bg(Color::Blue)
        .add_modifier(Modifier::BOLD)
}

/// Returns the terminal label for an evidence family.
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
