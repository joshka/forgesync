//! # Draw archive-wide evidence coverage and health
//!
//! `draw_coverage` presents the loaded `ArchiveStatus`: discussion and repository counts, resource
//! family completeness, schema history, writer lease, and unresolved durable work. This projection
//! describes the archive, not only the currently selected discussion or browser page.
//!
//! Completeness comes from stored evidence and freshness rules rather than visible child counts.
//! The screen helps readers distinguish absent content from acquisition that is missing or partial,
//! and identify recorded work that needs inspection or retry.
//!
//! `app/coverage` owns the last successful projection and refresh generation. A pending initial
//! read shows loading; a refresh with cached data keeps that projection and marks it as refreshing.
//! Current errors take precedence over cached content. Query tasks perform read-only inspection;
//! this renderer starts no work and does not decide whether an archive should sync or migrate.
//!
//! A borrowed `CoverageView` assembles identity, evidence, health, and lease sections from that one
//! loaded projection. Section methods own wording and emphasis; `draw_coverage` owns loading/error
//! precedence and placement in the frame.

use forgesync_store::reads::ArchiveStatus;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::App;
use crate::view::{family_name, pane};

/// Draws archive-wide coverage and health, marking cached refreshes and current read failures.
pub fn draw_coverage(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let block = pane("Archive coverage and health", true);
    let mut lines = if app.coverage.loading && app.coverage.data.is_none() {
        vec![Line::from("Loading archive coverage…")]
    } else if let Some(error) = &app.coverage.error {
        vec![Line::from(error.clone())]
    } else if let Some(status) = &app.coverage.data {
        CoverageView { status }.lines()
    } else {
        vec![Line::from("Press c to load coverage.")]
    };
    if app.coverage.loading && app.coverage.data.is_some() {
        lines.insert(0, Line::from("Refreshing…"));
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// Borrowed archive projection whose sections share one observed status.
///
/// This context owns presentation only. It keeps identity, evidence, and health formatting locally
/// navigable without creating a second status model or acquiring fresh diagnostics while drawing.
struct CoverageView<'a> {
    /// Last successfully loaded status, retained by the coverage panel during refresh.
    status: &'a ArchiveStatus,
}

impl CoverageView<'_> {
    /// Assembles archive identity, evidence, and local health in their terminal reading order.
    fn lines(&self) -> Vec<Line<'static>> {
        let mut lines = self.identity_lines();
        lines.extend(self.evidence_lines());
        lines.extend(self.health_lines());
        lines.extend(self.lease_lines());
        lines
    }

    /// Identifies the archive and its discussion totals before showing per-family evidence.
    fn identity_lines(&self) -> Vec<Line<'static>> {
        let status = self.status;
        vec![
            Line::from(format!("Archive {}", status.archive.archive_id)),
            Line::from(format!(
                "Schema {} · SQLite {}",
                status.archive.schema_version, status.archive.sqlite_version
            )),
            Line::from(format!(
                "{} repositories · {} discussions ({} issues, {} pull requests)",
                status.repositories, status.threads, status.issues, status.pull_requests
            )),
        ]
    }

    /// Preserves the store's family order and distinguishes incomplete from absent evidence.
    fn evidence_lines(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from(""),
            Line::from("Evidence coverage").style(Style::default().add_modifier(Modifier::BOLD)),
        ];
        lines.extend(self.status.coverage.iter().map(|coverage| {
            Line::from(format!(
                "{} · complete {} · incomplete {} · missing {} / {}",
                family_name(coverage.family),
                coverage.complete,
                coverage.incomplete,
                coverage.missing,
                coverage.applicable_threads
            ))
        }));
        lines
    }

    /// Displays durable work independently of source-family completeness.
    fn health_lines(&self) -> Vec<Line<'static>> {
        let work = &self.status.diagnostics.work;
        vec![
            Line::from(""),
            Line::from("Local health").style(Style::default().add_modifier(Modifier::BOLD)),
            Line::from(format!(
                "{} unresolved failures · {} failed jobs · {} deferred jobs · {} in-progress runs",
                work.unresolved_failures,
                work.failed_jobs,
                work.deferred_jobs,
                work.in_progress_runs
            )),
        ]
    }

    /// Shows lease owner and expiry only while the observed lease is held.
    ///
    /// Availability is diagnostic information, not permission for a later write. Missing owner or
    /// unformattable expiry remains explicit instead of implying that the lease is available.
    fn lease_lines(&self) -> Vec<Line<'static>> {
        let lease = &self.status.diagnostics.lease;
        if !lease.held {
            return vec![Line::from("Writer lease: available")];
        }
        let owner = lease.owner_id.as_deref().unwrap_or("unknown");
        let expires_at = lease
            .expires_at
            .format_rfc3339()
            .unwrap_or_else(|_| "unknown expiry".to_owned());
        vec![
            Line::from("Writer lease: held"),
            Line::from(format!("Writer owner: {owner} · expires {expires_at}")),
        ]
    }
}

#[cfg(test)]
mod tests {
    //! Local health rendering preserves its heading, counters, and available-lease wording.
    //!
    //! An explicit new archive supplies real diagnostics rather than a parallel fixture model.
    //! Store tests own diagnostic acquisition; this case protects its presentation as terminal
    //! lines.

    use forgesync_store::archive::Archive;
    use ratatui::style::{Modifier, Style};
    use ratatui::text::Line;

    use crate::view::coverage::CoverageView;

    #[tokio::test]
    async fn empty_archive_health_keeps_work_counts_before_available_lease() {
        let path = std::env::temp_dir().join(format!(
            "forgesync-tui-coverage-view-{}.sqlite",
            std::process::id()
        ));
        let archive = Archive::create(&path)
            .await
            .expect("create display archive");
        let status = archive.archive_status().await.expect("read display status");
        let view = CoverageView { status: &status };
        assert_eq!(
            view.health_lines(),
            vec![
                Line::from(""),
                Line::from("Local health").style(Style::default().add_modifier(Modifier::BOLD)),
                Line::from(
                    "0 unresolved failures · 0 failed jobs · 0 deferred jobs · 0 in-progress runs"
                ),
            ]
        );
        assert_eq!(
            view.lease_lines(),
            vec![Line::from("Writer lease: available")]
        );
        archive.close().await;
        std::fs::remove_file(path).expect("remove display archive");
    }
}
