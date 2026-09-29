//! # Draw repository and discussion browsing
//!
//! `draw_browser` arranges the current repository, thread list, and selected discussion detail in
//! terminal space. It uses `App` selections and loaded projections to show where the user is in
//! the archive.
//!
//! The engine constructs the underlying offline view; this module chooses layout, labels, and
//! focus cues. It should render loading and failure state from the app rather than silently
//! implying an empty archive.

use super::{
    App, COMPACT_WIDTH, Color, Constraint, Direction, Focus, Frame, Layout, Line, List, ListItem,
    ListState, Modifier, Paragraph, Rect, SourceState, Style, Text, ThreadDetail, ThreadKind,
    ThreadTimelineEvent, Wrap, family_name, pane_block, selected_style,
};

/// Arranges repository, discussion, and detail panes for the available width.
pub fn draw_browser(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
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

/// Draws registered repositories with the active picker selection.
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

/// Draws the current discussion page and selected row.
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

/// Draws selected discussion content or its loading and failure state.
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

/// Builds the ordered summary and timeline lines for one discussion.
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

/// Formats one archive timeline event for the detail pane.
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
