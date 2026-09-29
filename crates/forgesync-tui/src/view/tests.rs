use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::clusters::{ClusterDetail, ClusterLifecycle, ClusterSummary};
use forgesync_store::reads::{ThreadDetail, ThreadSummary};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

use super::draw;
use crate::app::{App, Screen};

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

#[test]
fn cluster_and_failure_views_render_at_narrow_and_wide_sizes() {
    let thread = sample_summary();
    let cluster = ClusterSummary {
        id: 3,
        repository: thread.repository.clone(),
        title: "Related discussions".to_owned(),
        lifecycle: ClusterLifecycle::Active,
        dismissed: false,
        dismissal_reason: None,
        representative: None,
        active_member_count: 1,
        excluded_member_count: 0,
        last_run_id: Some(1),
        updated_at: thread.discussion.updated_at,
    };
    for (screen, width, height) in [
        (Screen::Clusters, 40, 10),
        (Screen::Clusters, 140, 40),
        (Screen::ClusterDetail, 40, 10),
        (Screen::ClusterDetail, 140, 40),
        (Screen::Failures, 40, 10),
        (Screen::Failures, 140, 40),
    ] {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let mut app = App {
            screen,
            clusters: vec![cluster.clone()],
            cluster_detail: Some(ClusterDetail {
                cluster: cluster.clone(),
                members: Vec::new(),
            }),
            ..App::default()
        };
        terminal
            .draw(|frame| draw(frame, &mut app))
            .expect("draw maintainer view");
    }
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
