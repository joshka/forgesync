use forgesync_core::{SourceState, ThreadKind};
use forgesync_engine::{ArchiveStatus, ThreadDetail, ThreadTimelineEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, Focus, Screen};

const COMPACT_WIDTH: u16 = 100;

pub(crate) fn draw(frame: &mut Frame<'_>, app: &mut App) {
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
    }
    draw_footer(frame, sections[2], app);
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let view = match app.screen {
        Screen::Browser => "Browse",
        Screen::Coverage => "Coverage",
        Screen::Failures => "Failures",
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

fn draw_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let text = if app.searching {
        format!(" Search: {}▏  Enter search · Esc cancel", app.search_input)
    } else if let Some(status) = &app.status {
        format!(" {status}  ·  q quit")
    } else {
        " Tab focus · Enter select · / search · c coverage · f failures · n/p page · q quit"
            .to_owned()
    };
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::Gray)),
        area,
    );
}

fn draw_browser(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    if area.width >= COMPACT_WIDTH {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(35),
                Constraint::Percentage(40),
            ])
            .split(area);
        draw_repositories(frame, columns[0], app);
        draw_threads(frame, columns[1], app);
        draw_detail(frame, columns[2], app);
    } else {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
            ])
            .split(area);
        draw_repositories(frame, rows[0], app);
        draw_threads(frame, rows[1], app);
        draw_detail(frame, rows[2], app);
    }
}

fn draw_repositories(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut items = vec![ListItem::new("All repositories")];
    items.extend(
        app.repositories
            .iter()
            .map(|repository| ListItem::new(repository.full_name.clone())),
    );
    if app.repositories_loading && app.repositories.is_empty() {
        items = vec![ListItem::new("Loading repositories…")];
    } else if let Some(error) = &app.repository_error {
        items = vec![ListItem::new(error.clone())];
    }
    let title = if let Some(index) = app.applied_repository {
        app.repositories
            .get(index)
            .map(|repository| format!("Repositories · {}", repository.full_name))
            .unwrap_or_else(|| "Repositories".to_owned())
    } else {
        "Repositories · all".to_owned()
    };
    let mut state = ListState::default();
    if app.repository_error.is_none() {
        state.select(Some(
            app.repository_cursor.min(items.len().saturating_sub(1)),
        ));
    }
    let block = pane_block(&title, app.focus == Focus::Repositories);
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(selected_style()),
        area,
        &mut state,
    );
}

fn draw_threads(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut items: Vec<ListItem<'_>> = app
        .threads
        .iter()
        .map(|thread| {
            let number = thread.discussion.id.number().get();
            ListItem::new(format!("#{} {}", number, thread.discussion.title))
        })
        .collect();
    if app.threads_loading && app.threads.is_empty() {
        items.push(ListItem::new("Loading discussions…"));
    } else if let Some(error) = &app.thread_error {
        items = vec![ListItem::new(error.clone())];
    } else if app.threads.is_empty() {
        items.push(ListItem::new("No discussions"));
    }
    let title = format!(
        "Discussions · {}{}",
        app.threads.len(),
        if app.threads_loading {
            " · loading"
        } else {
            ""
        }
    );
    let mut state = ListState::default();
    if app.thread_error.is_none() && !app.threads.is_empty() {
        state.select(app.selected_thread);
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(pane_block(&title, app.focus == Focus::Threads))
            .highlight_style(selected_style()),
        area,
        &mut state,
    );
}

fn draw_detail(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let lines = detail_lines(
        app.detail.as_ref(),
        app.detail_loading,
        app.detail_error.as_deref(),
    );
    let block = pane_block("Discussion detail", app.focus == Focus::Detail);
    let inner_height = block.inner(area).height;
    let max_scroll = lines.len().saturating_sub(usize::from(inner_height));
    app.detail_scroll = app
        .detail_scroll
        .min(u16::try_from(max_scroll).unwrap_or(u16::MAX));
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0)),
        area,
    );
}

fn detail_lines(
    detail: Option<&ThreadDetail>,
    loading: bool,
    error: Option<&str>,
) -> Vec<Line<'static>> {
    let Some(detail) = detail else {
        let text = if loading {
            "Loading selected discussion…"
        } else if let Some(error) = error {
            return vec![Line::from(error.to_owned())];
        } else {
            "Select a discussion and press Enter to inspect it."
        };
        return vec![Line::from(text)];
    };
    let summary = &detail.summary;
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
        Line::from(discussion.title.clone()).style(Style::default().add_modifier(Modifier::BOLD)),
    ];
    if let Some(url) = &discussion.html_url {
        lines.push(Line::from(url.clone()).style(Style::default().fg(Color::Blue)));
    }
    lines.push(Line::from(""));
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
    lines.push(Line::from("Coverage").style(Style::default().add_modifier(Modifier::BOLD)));
    lines.extend(summary.coverage.iter().map(|coverage| {
        Line::from(format!(
            "{}: {:?}{}",
            family_name(coverage.family()),
            coverage.state(),
            if coverage.is_stale() { " (stale)" } else { "" }
        ))
    }));
    lines.push(Line::from(""));
    lines.push(Line::from("Current evidence").style(Style::default().add_modifier(Modifier::BOLD)));
    lines.extend(
        detail
            .timeline
            .iter()
            .map(|entry| timeline_line(&entry.event)),
    );
    lines
}

fn timeline_line(event: &ThreadTimelineEvent) -> Line<'static> {
    match event {
        ThreadTimelineEvent::ThreadCreated { title, .. } => Line::from(format!("Created: {title}")),
        ThreadTimelineEvent::ThreadClosed { .. } => Line::from("Discussion closed"),
        ThreadTimelineEvent::Comment { comment } => Line::from(format!(
            "Comment · {}: {}",
            comment.author.as_deref().unwrap_or("unknown author"),
            comment.body
        )),
        ThreadTimelineEvent::Review { review } => Line::from(format!(
            "Review · {} · {}",
            review
                .reviewer
                .as_ref()
                .and_then(|reviewer| reviewer.login.as_deref())
                .unwrap_or("unknown reviewer"),
            review.body.as_deref().unwrap_or("(no review comment)")
        )),
        ThreadTimelineEvent::ReviewThread {
            is_resolved,
            is_outdated,
            path,
            ..
        } => Line::from(format!(
            "Review thread · {} · {} · {}",
            if *is_resolved {
                "resolved"
            } else {
                "unresolved"
            },
            if *is_outdated { "outdated" } else { "current" },
            path.as_deref().unwrap_or("unknown path")
        )),
        ThreadTimelineEvent::ReviewThreadComment { comment, path, .. } => Line::from(format!(
            "Review comment · {} · {}: {}",
            path.as_deref().unwrap_or("unknown path"),
            comment.author.as_deref().unwrap_or("unknown author"),
            comment.body
        )),
    }
}

fn draw_coverage(frame: &mut Frame<'_>, area: Rect, app: &App) {
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
                "held by another or active process"
            } else {
                "available"
            }
        )),
    ]);
    lines
}

fn draw_failures(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = if app.failures_loading && app.failures.is_empty() {
        vec![Line::from("Loading recent run failures…")]
    } else if let Some(error) = &app.failures_error {
        vec![Line::from(error.clone())]
    } else if app.failures.is_empty() {
        vec![Line::from(
            "No incomplete or failed runs in the recent run window.",
        )]
    } else {
        let mut lines = Vec::new();
        for run in &app.failures {
            lines.push(
                Line::from(format!("Run #{} · {:?}", run.id, run.status)).style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            );
            if run.entries.is_empty() {
                lines.push(Line::from("  No unresolved failure details recorded."));
            } else {
                lines.extend(
                    run.entries
                        .iter()
                        .map(|entry| Line::from(format!("  {entry}"))),
                );
            }
            lines.push(Line::from(""));
        }
        lines
    };
    if app.failures_loading && !app.failures.is_empty() {
        lines.insert(0, Line::from("Refreshing…"));
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(pane_block("Recent failures and unfinished work", true))
            .wrap(Wrap { trim: false }),
        area,
    );
}

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

fn selected_style() -> Style {
    Style::default()
        .fg(Color::White)
        .bg(Color::Blue)
        .add_modifier(Modifier::BOLD)
}

fn family_name(family: forgesync_core::EvidenceFamily) -> &'static str {
    match family {
        forgesync_core::EvidenceFamily::Threads => "threads",
        forgesync_core::EvidenceFamily::Comments => "comments",
        forgesync_core::EvidenceFamily::PullRequestMetadata => "pull request metadata",
        forgesync_core::EvidenceFamily::Reviews => "reviews",
        forgesync_core::EvidenceFamily::ReviewThreads => "review threads",
    }
}

#[cfg(test)]
mod tests {
    use forgesync_core::{
        Discussion, GitHubHost, ProviderData, ProviderId, Repository, RepositoryId, SourceState,
        ThreadId, ThreadKind, ThreadNumber, UtcTimestamp,
    };
    use forgesync_store::{ThreadDetail, ThreadSummary};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    use crate::app::App;

    use super::draw;

    #[test]
    fn browser_renders_in_small_and_large_terminal_sizes() {
        for (width, height) in [(40, 10), (140, 40)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).expect("test terminal");
            let mut app = sample_app();
            terminal
                .draw(|frame| draw(frame, &mut app))
                .expect("draw browser");
        }
    }

    #[test]
    fn resizing_clamps_detail_scroll_to_visible_content() {
        let backend = TestBackend::new(50, 14);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut app = sample_app();
        app.detail = Some(sample_detail());
        app.focus = crate::app::Focus::Detail;
        app.detail_scroll = u16::MAX;
        terminal
            .draw(|frame| draw(frame, &mut app))
            .expect("draw small browser");
        let small_offset = app.detail_scroll;

        terminal
            .resize(Rect::new(0, 0, 140, 40))
            .expect("resize terminal");
        terminal
            .draw(|frame| draw(frame, &mut app))
            .expect("draw resized browser");
        assert!(app.detail_scroll <= small_offset);
    }

    fn sample_app() -> App {
        let summary = sample_summary();
        let mut app = App {
            repositories: vec![summary.repository.clone()],
            threads: vec![summary.clone()],
            selected_thread: Some(0),
            ..App::default()
        };
        app.applied_repository = Some(0);
        app.repository_cursor = 1;
        app
    }

    fn sample_detail() -> ThreadDetail {
        ThreadDetail {
            summary: sample_summary(),
            comments: Vec::new(),
            pull_request_metadata: Vec::new(),
            reviews: Vec::new(),
            review_threads: Vec::new(),
            timeline: Vec::new(),
        }
    }

    fn sample_summary() -> ThreadSummary {
        let repository = Repository {
            id: RepositoryId::new(
                GitHubHost::parse("github.com").expect("host"),
                ProviderId::new("41").expect("repository ID"),
            ),
            owner: "owner".to_owned(),
            name: "repo".to_owned(),
            full_name: "owner/repo".to_owned(),
            default_branch: Some("main".to_owned()),
            updated_at: None,
            provider_data: ProviderData::new(),
        };
        let timestamp = UtcTimestamp::parse("2026-09-29T00:00:00Z").expect("timestamp");
        let discussion = Discussion {
            id: ThreadId::new(
                repository.id.clone(),
                ProviderId::new("1001").expect("thread ID"),
                ThreadNumber::new(7).expect("thread number"),
            ),
            kind: ThreadKind::Issue,
            state: SourceState::Open,
            title: "Sample discussion".to_owned(),
            body: Some("Long detail body for scroll testing.".to_owned()),
            html_url: Some("https://github.com/owner/repo/issues/7".to_owned()),
            created_at: timestamp,
            updated_at: timestamp,
            closed_at: None,
            labels: vec!["triage".to_owned()],
            assignees: Vec::new(),
            provider_data: ProviderData::new(),
        };
        ThreadSummary {
            repository,
            discussion,
            coverage: Vec::new(),
        }
    }
}
