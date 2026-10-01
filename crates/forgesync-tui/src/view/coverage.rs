//! Archive-wide evidence coverage, local health, and the writer lease.

use forgesync_store::reads::ArchiveStatus;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::App;
use crate::view::{family_name, pane};

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

/// Sections of one loaded archive status.
struct CoverageView<'a> {
    status: &'a ArchiveStatus,
}

impl CoverageView<'_> {
    fn lines(&self) -> Vec<Line<'static>> {
        let mut lines = self.identity_lines();
        lines.extend(self.evidence_lines());
        lines.extend(self.health_lines());
        lines.extend(self.lease_lines());
        lines
    }

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

    /// Distinguishes incomplete from missing evidence per family.
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

    /// A held lease with an unknown owner or expiry says so rather than looking available.
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
