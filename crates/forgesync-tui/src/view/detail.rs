//! Discussion detail lines, also used by the browser to bound scrolling.

use forgesync_core::content::{SourceState, ThreadKind};
use forgesync_store::reads::ThreadDetail;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;

use crate::app::App;
use crate::view::family_name;
use crate::view::timeline::timeline_line;

/// The detail pane's lines, or one placeholder line while loading, failed, or unselected.
pub fn detail_lines(app: &App) -> Vec<Line<'static>> {
    let pane = &app.detail_pane.detail;
    let Some(detail) = pane.data.as_deref() else {
        let text = if pane.loading {
            "Loading selected discussion…"
        } else if let Some(error) = &pane.error {
            return vec![Line::from(error.clone())];
        } else {
            "Select a discussion and press Enter to inspect it."
        };
        return vec![Line::from(text)];
    };
    DetailPresentation(detail).lines()
}

/// Sections of one loaded discussion.
struct DetailPresentation<'a>(&'a ThreadDetail);
impl DetailPresentation<'_> {
    fn lines(&self) -> Vec<Line<'static>> {
        let mut lines = self.source_lines();
        lines.extend(self.body_lines());
        lines.extend(self.coverage_lines());
        lines.extend(self.timeline_lines());
        lines
    }
    fn source_lines(&self) -> Vec<Line<'static>> {
        let summary = &self.0.summary;
        let discussion = &summary.discussion;
        let kind = match discussion.kind {
            ThreadKind::Issue => "Issue",
            ThreadKind::PullRequest => "Pull request",
        };
        let state = match &discussion.state {
            SourceState::Open => "open".to_owned(),
            SourceState::Closed => "closed".to_owned(),
            SourceState::Other(value) => value.clone(),
        };
        let mut lines = vec![
            Line::from(format!("{} #{}", kind, discussion.id.number().get())).style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::from(summary.repository.full_name.clone()),
            Line::from(state),
            Line::from(discussion.title.clone())
                .style(Style::default().add_modifier(Modifier::BOLD)),
        ];
        if let Some(url) = &discussion.html_url {
            lines.push(Line::from(url.clone()).style(Style::default().fg(Color::Blue)));
        }
        lines.push(Line::from(""));
        lines
    }
    fn body_lines(&self) -> Vec<Line<'static>> {
        let discussion = &self.0.summary.discussion;
        let mut lines = Vec::new();
        if let Some(body) = &discussion.body {
            lines.push(Line::from("Body").style(Style::default().add_modifier(Modifier::BOLD)));
            lines.extend(body.lines().map(|line| Line::from(line.to_owned())));
            lines.push(Line::from(""));
        }
        if !discussion.labels.is_empty() {
            lines.push(Line::from(format!(
                "Labels: {}",
                discussion.labels.join(", ")
            )));
        }
        lines
    }
    fn coverage_lines(&self) -> Vec<Line<'static>> {
        let summary = &self.0.summary;
        let mut lines = Vec::new();
        lines.push(Line::from("Coverage").style(Style::default().add_modifier(Modifier::BOLD)));
        lines.extend(summary.coverage.iter().map(|coverage| {
            Line::from(format!(
                "{}: {:?}{}",
                family_name(coverage.family()),
                coverage.state(),
                if coverage.is_stale() { " (stale)" } else { "" }
            ))
        }));
        lines
    }
    fn timeline_lines(&self) -> Vec<Line<'static>> {
        let detail = self.0;
        let mut lines = Vec::new();
        lines.push(Line::from(""));
        lines.push(
            Line::from("Current evidence").style(Style::default().add_modifier(Modifier::BOLD)),
        );
        lines.extend(
            detail
                .timeline
                .iter()
                .map(|entry| timeline_line(&entry.event)),
        );
        lines
    }
}
