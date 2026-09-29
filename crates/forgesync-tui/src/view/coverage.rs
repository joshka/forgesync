//! # Draw acquired evidence coverage
//!
//! `draw_coverage` presents which discussion resource families are complete, partial, or missing
//! for the selected thread. It uses explicit stored coverage instead of inferring completeness
//! from visible child counts.
//!
//! This screen helps a reader decide whether missing content is genuinely absent or simply not yet
//! acquired. Data loading lives in `query/reads`, and selection lives in `App`.

use super::{
    App, ArchiveStatus, Frame, Line, Modifier, Paragraph, Rect, Style, Text, Wrap, family_name,
    pane_block,
};

/// Draws family coverage for the currently selected archive scope.
pub fn draw_coverage(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let block = pane_block("Archive coverage and health", true);
    let mut lines = if app.coverage_loading && app.coverage.is_none() {
        vec![Line::from("Loading archive coverage…")]
    } else if let Some(error) = &app.coverage_error {
        vec![Line::from(error.clone())]
    } else if let Some(status) = &app.coverage {
        coverage_lines(status)
    } else {
        vec![Line::from("Press c to load coverage.")]
    };
    if app.coverage_loading && app.coverage.is_some() {
        lines.insert(0, Line::from("Refreshing…"));
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// Converts coverage counts and state into ordered terminal lines.
fn coverage_lines(status: &ArchiveStatus) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(format!("Archive {}", status.archive.archive_id)),
        Line::from(format!(
            "Schema {} · SQLite {}",
            status.archive.schema_version, status.archive.sqlite_version
        )),
        Line::from(format!(
            "{} repositories · {} discussions ({} issues, {} pull requests)",
            status.repositories, status.threads, status.issues, status.pull_requests
        )),
        Line::from(""),
        Line::from("Evidence coverage").style(Style::default().add_modifier(Modifier::BOLD)),
    ];
    lines.extend(status.coverage.iter().map(|coverage| {
        Line::from(format!(
            "{} · complete {} · incomplete {} · missing {} / {}",
            family_name(coverage.family),
            coverage.complete,
            coverage.incomplete,
            coverage.missing,
            coverage.applicable_threads
        ))
    }));
    let work = &status.diagnostics.work;
    lines.extend([
        Line::from(""),
        Line::from("Local health").style(Style::default().add_modifier(Modifier::BOLD)),
        Line::from(format!(
            "{} unresolved failures · {} failed jobs · {} deferred jobs · {} in-progress runs",
            work.unresolved_failures, work.failed_jobs, work.deferred_jobs, work.in_progress_runs
        )),
        Line::from(format!(
            "Writer lease: {}",
            if status.diagnostics.lease.held {
                "held"
            } else {
                "available"
            }
        )),
    ]);
    if status.diagnostics.lease.held {
        let expires_at = status
            .diagnostics
            .lease
            .expires_at
            .format_rfc3339()
            .unwrap_or_else(|_| "unknown expiry".to_owned());
        lines.push(Line::from(format!(
            "Writer owner: {} · expires {expires_at}",
            status
                .diagnostics
                .lease
                .owner_id
                .as_deref()
                .unwrap_or("unknown")
        )));
    }
    lines
}
