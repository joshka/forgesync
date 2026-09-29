//! # Detail selection and asynchronous read transitions
//!
//! These cases protect current-selection display rather than archive acquisition. Starting a
//! read removes prior errors; successful content begins at the top; failed content is unavailable;
//! and invalidation prevents an older selection from returning through a late reply.
//!
//! The fixture is one explicit discussion with empty evidence collections. Each scenario changes
//! only generation, presentation, or scrolling. Query/provider and viewport behavior stay in their
//! separate workflow and rendering tests.

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::reads::{ThreadDetail, ThreadSummary};
use rstest::{fixture, rstest};

use super::{DetailPane, DetailState};

#[test]
fn beginning_a_read_removes_prior_error_and_scroll_intent() {
    let mut pane = DetailPane {
        state: DetailState::Failed("previous failure".to_owned()),
        scroll: 42,
        ..Default::default()
    };
    assert_eq!(pane.begin(), 1);
    assert!(pane.is_loading());
    assert_eq!(pane.error(), None);
    assert_eq!(pane.content(), None);
    assert_eq!(pane.scroll, 0);
}

#[rstest]
fn current_success_resets_scrolling_and_exposes_loaded_content(detail: ThreadDetail) {
    let mut pane = DetailPane::default();
    assert_eq!(pane.begin(), 1);
    pane.scroll = 42;
    assert_eq!(pane.apply(1, Ok(Box::new(detail))), None);
    assert!(!pane.is_loading());
    assert_eq!(pane.error(), None);
    assert_eq!(
        pane.content()
            .expect("current detail")
            .summary
            .discussion
            .title,
        "Selected discussion"
    );
    assert_eq!(pane.scroll, 0);
}

#[test]
fn failed_read_has_no_content_or_loading_state() {
    let mut pane = DetailPane::default();
    assert_eq!(pane.begin(), 1);
    assert_eq!(
        pane.apply(1, Err("read failed".to_owned())).as_deref(),
        Some("read failed")
    );
    assert_eq!(pane.error(), Some("read failed"));
    assert_eq!(pane.content(), None);
    assert!(!pane.is_loading());
}

#[test]
fn stale_failure_cannot_finish_a_newer_detail_read() {
    let mut pane = DetailPane::default();
    assert_eq!(pane.begin(), 1);
    assert_eq!(pane.begin(), 2);
    assert_eq!(pane.apply(1, Err("stale failure".to_owned())), None);
    assert_eq!(pane.generation, 2);
    assert!(pane.is_loading());
    assert_eq!(pane.error(), None);
}

#[rstest]
fn invalidating_selection_prevents_its_late_success_from_restoring_content(detail: ThreadDetail) {
    let mut pane = DetailPane {
        generation: 2,
        scroll: 42,
        state: DetailState::Ready(Box::new(detail.clone())),
    };
    pane.invalidate();
    assert_eq!(pane.generation, 3);
    assert_eq!(pane.content(), None);
    assert_eq!(pane.scroll, 0);
    assert_eq!(pane.apply(2, Ok(Box::new(detail))), None);
    assert_eq!(pane.content(), None);
    assert!(!pane.is_loading());
}

/// A selected discussion projection whose empty child evidence does not affect pane transitions.
#[fixture]
fn detail() -> ThreadDetail {
    let repository = Repository {
        id: RepositoryId::new(
            GitHubHost::parse("github.com").expect("host"),
            ProviderId::new("41").expect("repository ID"),
        ),
        owner: "owner".to_owned(),
        name: "repo".to_owned(),
        full_name: "owner/repo".to_owned(),
        default_branch: None,
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
        title: "Selected discussion".to_owned(),
        body: None,
        html_url: None,
        created_at: timestamp,
        updated_at: timestamp,
        closed_at: None,
        labels: Vec::new(),
        assignees: Vec::new(),
        provider_data: ProviderData::new(),
    };
    ThreadDetail {
        summary: ThreadSummary {
            repository,
            discussion,
            coverage: Vec::new(),
        },
        comments: Vec::new(),
        pull_request_metadata: Vec::new(),
        reviews: Vec::new(),
        review_threads: Vec::new(),
        timeline: Vec::new(),
    }
}
