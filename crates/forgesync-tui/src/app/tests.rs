//! # Interactive state transitions
//!
//! These tests demonstrate that keyboard input remains responsive while queries run, and stale
//! results cannot overwrite a newer selection. They also cover search entry, repository picking,
//! and targeting the selected cluster member. `App` is the state machine behind the view; these
//! examples show its user-facing transitions without requiring a terminal renderer. Add a direct
//! transition case when a new key changes navigation or launches work.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::identity::{
    GitHubHost, ProviderId, RepositoryId, RunId, ThreadId, ThreadNumber,
};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_engine::sync::{SyncProgress, SyncProgressStatus};
use forgesync_store::clusters::{
    ClusterDetail, ClusterLifecycle, ClusterMember, ClusterMemberRole, ClusterMemberState,
    ClusterSummary,
};
use forgesync_store::reads::{ThreadPage, ThreadSummary};

use super::{App, Focus, QueryAction, QueryMessage, Screen};

#[test]
fn keyboard_input_remains_available_while_queries_are_pending() {
    let mut app = App {
        threads_loading: true,
        ..App::default()
    };

    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.focus, Focus::Threads);
    app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(app.quit);
}

#[test]
fn stale_thread_result_does_not_replace_current_query_state() {
    let mut app = App {
        thread_generation: 2,
        threads_loading: true,
        ..App::default()
    };
    app.apply(QueryMessage::Threads {
        generation: 1,
        offset: 0,
        result: Ok(Box::new(ThreadPage {
            items: Vec::new(),
            next_offset: None,
            coverage: Vec::new(),
        })),
    });

    assert!(app.threads_loading);
    assert_eq!(app.thread_generation, 2);
    assert!(app.threads.is_empty());
}

#[test]
fn search_keys_build_a_local_query_until_enter() {
    let mut app = App {
        searching: true,
        ..App::default()
    };
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
    assert!(app.search_query.is_none());

    let actions = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.search_query.as_deref(), Some("tui search"));
    assert_eq!(actions.len(), 1);
}

#[test]
fn repository_picker_applies_the_highlighted_repository() {
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
    let mut app = App {
        repositories: vec![repository],
        repository_cursor: 1,
        ..App::default()
    };

    let actions = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.applied_repository, Some(0));
    assert!(matches!(app.screen, Screen::Browser));
    let [
        QueryAction::Threads {
            repositories,
            offset: 0,
            ..
        },
    ] = actions.as_slice()
    else {
        panic!("expected a repository-scoped thread query");
    };
    assert_eq!(repositories.len(), 1);
    assert_eq!(repositories[0].as_url(), "https://github.com/owner/repo");
}

#[test]
fn opening_keyword_search_returns_to_the_browser() {
    let mut app = App {
        screen: Screen::Failures,
        ..App::default()
    };

    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));

    assert!(app.searching);
    assert_eq!(app.screen, Screen::Browser);
}

#[test]
fn maintainer_keys_target_the_selected_cluster_member() {
    let mut app = App {
        screen: Screen::ClusterDetail,
        cluster_detail: Some(sample_cluster_detail()),
        ..App::default()
    };

    let exclude = app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
    assert!(matches!(
        exclude.as_slice(),
        [QueryAction::SetClusterMemberExcluded {
            id: 17,
            excluded: true,
            ..
        }]
    ));

    let canonical = app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
    assert!(matches!(
        canonical.as_slice(),
        [QueryAction::SetCanonicalClusterMember { id: 17, .. }]
    ));
}

#[test]
fn writer_actions_use_current_repository_scope() {
    let repository = sample_repository();
    let mut app = App {
        repositories: vec![repository],
        applied_repository: Some(0),
        ..App::default()
    };

    let sync = app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    assert!(matches!(
        sync.as_slice(),
        [QueryAction::Sync { repositories }] if repositories.len() == 1
    ));
    let refresh = app.handle_key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::NONE));
    assert!(matches!(
        refresh.as_slice(),
        [QueryAction::Refresh { repositories }] if repositories.len() == 1
    ));
}

#[test]
fn cluster_dismiss_and_selected_run_retry_use_the_current_selection() {
    let detail = sample_cluster_detail();
    let mut app = App {
        screen: Screen::Clusters,
        clusters: vec![detail.cluster],
        failures: vec![super::RunFailureSummary {
            id: 23,
            status: super::RunStatus::Failed,
            entries: vec!["owner/repo: threads failed".to_owned()],
        }],
        ..App::default()
    };
    let dismiss = app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
    assert!(matches!(
        dismiss.as_slice(),
        [QueryAction::DismissCluster {
            id: 17,
            dismissed: true,
        }]
    ));

    app.screen = Screen::Failures;
    let retry = app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
    assert!(matches!(
        retry.as_slice(),
        [QueryAction::Retry(run_id)] if *run_id == RunId::new(23).expect("run ID")
    ));
}

#[test]
fn quit_cancels_active_action_and_failed_result_stays_visible() {
    let mut app = App {
        operation_generation: 4,
        operation_busy: true,
        operation_label: Some("sync".to_owned()),
        ..App::default()
    };

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(matches!(action.as_slice(), [QueryAction::CancelOperation]));
    assert!(!app.quit);

    app.apply(QueryMessage::OperationFinished {
        generation: 4,
        result: Err("archive writer lease is held".to_owned()),
    });
    assert!(!app.operation_busy);
    assert_eq!(
        app.status.as_deref(),
        Some("Failed: archive writer lease is held")
    );
    app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(app.quit);
}

#[test]
fn stale_operation_progress_cannot_replace_current_progress() {
    let mut app = App {
        operation_generation: 3,
        operation_busy: true,
        ..App::default()
    };
    let progress = SyncProgress {
        run_id: RunId::new(1).expect("positive run ID"),
        completed_jobs: 1,
        total_jobs: 2,
        threads_seen: 3,
        comments_seen: 4,
        pull_request_metadata_seen: 5,
        reviews_seen: 6,
        review_threads_seen: 7,
        repository: Some("https://github.com/owner/repo".to_owned()),
        status: SyncProgressStatus::InProgress,
    };

    app.apply(QueryMessage::OperationProgress {
        generation: 2,
        progress,
    });
    assert!(app.operation_progress.is_none());
}

fn sample_repository() -> Repository {
    Repository {
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
    }
}

fn sample_cluster_detail() -> ClusterDetail {
    let repository = sample_repository();
    let timestamp = UtcTimestamp::parse("2026-09-29T00:00:00Z").expect("timestamp");
    let discussion = Discussion {
        id: ThreadId::new(
            repository.id.clone(),
            ProviderId::new("1001").expect("thread ID"),
            ThreadNumber::new(7).expect("thread number"),
        ),
        kind: ThreadKind::Issue,
        state: SourceState::Open,
        title: "Selected neighbor".to_owned(),
        body: None,
        html_url: None,
        created_at: timestamp,
        updated_at: timestamp,
        closed_at: None,
        labels: Vec::new(),
        assignees: Vec::new(),
        provider_data: ProviderData::new(),
    };
    ClusterDetail {
        cluster: ClusterSummary {
            id: 17,
            repository: repository.clone(),
            title: "Cluster title".to_owned(),
            lifecycle: ClusterLifecycle::Active,
            dismissed: false,
            dismissal_reason: None,
            representative: None,
            active_member_count: 1,
            excluded_member_count: 0,
            last_run_id: Some(2),
            updated_at: timestamp,
        },
        members: vec![ClusterMember {
            summary: ThreadSummary {
                repository,
                discussion,
                coverage: Vec::new(),
            },
            role: ClusterMemberRole::Representative,
            state: ClusterMemberState::Active,
            score_to_representative: Some(1.0),
        }],
    }
}
