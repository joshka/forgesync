//! Rendering at narrow and wide sizes, focus borders, and detail scroll clamping.

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::clusters::{ClusterDetail, ClusterLifecycle, ClusterSummary};
use forgesync_store::reads::{ThreadDetail, ThreadSummary};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::{ListState, Widget};

use super::draw;
use crate::app::loadable::Loadable;
use crate::app::panels::{ClusterDetailPane, ClusterList, RepositoryPicker, ThreadList};
use crate::app::{App, Focus, Screen};
use crate::view::pane;

#[rstest::rstest]
#[case::repositories_focused(Focus::Repositories, Focus::Repositories, Color::Cyan)]
#[case::threads_focused(Focus::Threads, Focus::Threads, Color::Cyan)]
#[case::detail_focused(Focus::Detail, Focus::Detail, Color::Cyan)]
#[case::repositories_unfocused(Focus::Threads, Focus::Repositories, Color::DarkGray)]
#[case::threads_unfocused(Focus::Detail, Focus::Threads, Color::DarkGray)]
#[case::detail_unfocused(Focus::Repositories, Focus::Detail, Color::DarkGray)]
fn pane_border_preserves_the_focus_palette(
    #[case] current: Focus,
    #[case] focus: Focus,
    #[case] expected: Color,
) {
    let area = Rect::new(0, 0, 8, 3);
    let mut buffer = Buffer::empty(area);
    let block = pane("Pane", current == focus);

    block.render(area, &mut buffer);

    assert_eq!(buffer[(0, 0)].fg, expected);
}

#[rstest::rstest]
#[case::narrow(40, 10)]
#[case::wide(140, 40)]
fn browser_renders_at_terminal_size(#[case] width: u16, #[case] height: u16) {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut app = sample_app();
    terminal
        .draw(|frame| draw(frame, &mut app))
        .expect("draw browser");
}

#[test]
fn resizing_clamps_detail_scroll_to_visible_content() {
    let backend = TestBackend::new(50, 14);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut app = sample_app();
    app.detail_pane.detail = Loadable::loaded(Some(Box::new(sample_detail())));
    app.focus = crate::app::Focus::Detail;
    app.detail_pane.scroll = u16::MAX;
    terminal
        .draw(|frame| draw(frame, &mut app))
        .expect("draw small browser");
    let small_offset = app.detail_pane.scroll;

    terminal
        .resize(Rect::new(0, 0, 140, 40))
        .expect("resize terminal");
    terminal
        .draw(|frame| draw(frame, &mut app))
        .expect("draw resized browser");
    assert!(app.detail_pane.scroll <= small_offset);
}

#[rstest::rstest]
#[case::clusters_narrow(Screen::Clusters, 40, 10)]
#[case::clusters_wide(Screen::Clusters, 140, 40)]
#[case::cluster_detail_narrow(Screen::ClusterDetail, 40, 10)]
#[case::cluster_detail_wide(Screen::ClusterDetail, 140, 40)]
#[case::failures_narrow(Screen::Failures, 40, 10)]
#[case::failures_wide(Screen::Failures, 140, 40)]
fn maintainer_view_renders_at_terminal_size(
    #[case] screen: Screen,
    #[case] width: u16,
    #[case] height: u16,
) {
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
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let mut app = App {
        screen,
        cluster_list: ClusterList {
            rows: Loadable::loaded(vec![cluster.clone()]),
            ..Default::default()
        },
        cluster_detail_pane: ClusterDetailPane {
            detail: Loadable::loaded(Some(ClusterDetail {
                cluster,
                members: Vec::new(),
            })),
            ..Default::default()
        },
        ..App::default()
    };
    terminal
        .draw(|frame| draw(frame, &mut app))
        .expect("draw maintainer view");
}

/// Row one is the repository, after the synthetic all-repositories row.
fn sample_app() -> App {
    let summary = sample_summary();
    let mut app = App {
        repository_picker: RepositoryPicker {
            rows: Loadable::loaded(vec![summary.repository.clone()]),
            ..Default::default()
        },
        thread_list: ThreadList {
            rows: Loadable::loaded(vec![summary.clone()]),
            state: ListState::default().with_selected(Some(0)),
            ..Default::default()
        },
        ..App::default()
    };
    app.repository_picker.applied = Some(summary.repository.clone());
    app.repository_picker.state.select(Some(1));
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
