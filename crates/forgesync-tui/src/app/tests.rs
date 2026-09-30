//! # Interactive state transitions
//!
//! These tests demonstrate that keyboard input remains responsive while queries run, and stale
//! results cannot overwrite a newer selection. They also cover search entry, repository picking,
//! and targeting the selected cluster member. `App` is the state machine behind the view; these
//! examples show its user-facing transitions without requiring a terminal renderer. Add a direct
//! transition case when a new key changes navigation or launches work.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use forgesync_core::content::Repository;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, RunId};
use forgesync_core::provider_data::ProviderData;
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};
use forgesync_engine::sync::{SyncProgress, SyncProgressStatus};
use forgesync_store::reads::ThreadPage;

use crate::app::messages::QueryMessage;
use crate::app::operation::{OperationDisplay, OperationState};
use crate::app::repositories::RepositoryPicker;
use crate::app::test_data::{sample_cluster_detail, sample_repository};
use crate::app::threads::{ThreadList, ThreadReply};
use crate::app::{App, Focus, Screen};
use crate::query::requests::QueryAction;

#[test]
fn keyboard_input_remains_available_while_queries_are_pending() {
    let mut app = App {
        thread_list: ThreadList {
            loading: true,
            ..ThreadList::default()
        },
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
        thread_list: ThreadList {
            generation: 2,
            loading: true,
            ..ThreadList::default()
        },
        ..App::default()
    };
    app.apply(QueryMessage::Threads(ThreadReply {
        generation: 1,
        offset: 0,
        result: Ok(Box::new(ThreadPage {
            items: Vec::new(),
            next_offset: None,
            coverage: Vec::new(),
        })),
    }));

    assert!(app.thread_list.loading);
    assert_eq!(app.thread_list.generation, 2);
    assert!(app.thread_list.items.is_empty());
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
        repository_picker: RepositoryPicker {
            items: vec![repository],
            cursor: 1,
            ..RepositoryPicker::default()
        },
        ..App::default()
    };

    let actions = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(
        app.repository_picker
            .applied
            .as_ref()
            .map(|repository| repository.full_name.as_str()),
        Some("owner/repo")
    );
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
fn exclude_targets_the_selected_cluster_member() {
    let mut app = App {
        screen: Screen::ClusterDetail,
        cluster_detail_pane: crate::app::clusters::ClusterDetailPane {
            data: Some(sample_cluster_detail()),
            ..Default::default()
        },
        ..App::default()
    };
    let reference = "owner/repo#7"
        .parse::<ThreadSelector>()
        .expect("member selector");

    let actions = app.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));

    assert_eq!(
        actions,
        [QueryAction::ExcludeClusterMember { id: 17, reference }]
    );
}

#[test]
fn canonical_targets_the_selected_cluster_member() {
    let mut app = App {
        screen: Screen::ClusterDetail,
        cluster_detail_pane: crate::app::clusters::ClusterDetailPane {
            data: Some(sample_cluster_detail()),
            ..Default::default()
        },
        ..App::default()
    };
    let reference = "owner/repo#7"
        .parse::<ThreadSelector>()
        .expect("member selector");

    let actions = app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));

    assert_eq!(
        actions,
        [QueryAction::SetCanonicalClusterMember { id: 17, reference }]
    );
}

#[test]
fn sync_uses_the_applied_repository_scope() {
    let repository = sample_repository();
    let mut app = App {
        repository_picker: RepositoryPicker {
            items: vec![repository.clone()],
            applied: Some(repository),
            ..RepositoryPicker::default()
        },
        ..App::default()
    };
    let repository = "owner/repo"
        .parse::<RepositorySelector>()
        .expect("selector");

    let actions = app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));

    assert_eq!(
        actions,
        [QueryAction::Sync {
            repositories: vec![repository]
        }]
    );
}

#[test]
fn refresh_uses_the_applied_repository_scope() {
    let repository = sample_repository();
    let mut app = App {
        repository_picker: RepositoryPicker {
            items: vec![repository.clone()],
            applied: Some(repository),
            ..RepositoryPicker::default()
        },
        ..App::default()
    };
    let repository = "owner/repo"
        .parse::<RepositorySelector>()
        .expect("selector");

    let actions = app.handle_key(KeyEvent::new(KeyCode::Char('R'), KeyModifiers::NONE));

    assert_eq!(
        actions,
        [QueryAction::Refresh {
            repositories: vec![repository]
        }]
    );
}

#[test]
fn dismissal_targets_the_selected_cluster() {
    let detail = sample_cluster_detail();
    let mut app = App {
        screen: Screen::Clusters,
        cluster_list: crate::app::clusters::ClusterList {
            items: vec![detail.cluster],
            ..Default::default()
        },
        ..App::default()
    };

    let actions = app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));

    assert_eq!(actions, [QueryAction::DismissCluster { id: 17 }]);
}

#[test]
fn retry_targets_the_selected_failed_run() {
    let mut app = App {
        screen: Screen::Failures,
        failure_list: crate::app::failures::FailureList {
            items: vec![crate::app::failures::RunFailureSummary {
                id: 23,
                status: forgesync_store::runs::RunStatus::Failed,
                entries: vec!["owner/repo: threads failed".to_owned()],
            }],
            ..Default::default()
        },
        ..App::default()
    };

    let actions = app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));

    assert_eq!(
        actions,
        [QueryAction::Retry(RunId::new(23).expect("run ID"))]
    );
}

#[test]
fn quit_cancels_active_action_and_failed_result_stays_visible() {
    let mut app = App {
        operation: OperationDisplay {
            generation: 4,
            state: OperationState::Running {
                label: "sync".to_owned(),
                progress: None,
            },
        },
        ..App::default()
    };

    let action = app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(matches!(action.as_slice(), [QueryAction::CancelOperation]));
    assert!(!app.quit);

    app.apply(QueryMessage::OperationFinished {
        generation: 4,
        result: Err("archive writer lease is held".to_owned()),
    });
    assert!(!app.operation.busy());
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
        operation: OperationDisplay {
            generation: 3,
            state: OperationState::Running {
                label: "sync".to_owned(),
                progress: None,
            },
        },
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
    assert!(app.operation.progress().is_none());
}
