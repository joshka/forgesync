//! # Discussion page transitions and stale replies
//!
//! These cases protect the list owner's local contract: beginning work clears obsolete results,
//! a successful page chooses a valid row and continuation, and failure cannot leave navigable old
//! rows. A late failure cannot finish the current request or replace its requested coordinates.
//!
//! The fixture contains one explicit archived discussion and prior pagination/error state. Tests
//! compare transitions directly; archive acquisition, filtering, and terminal drawing are covered
//! by engine/store and view suites rather than recreated here.

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::reads::{ThreadPage, ThreadSummary};
use rstest::{fixture, rstest};

use super::{ThreadList, ThreadReply};

/// One loaded discussion with prior selection, continuation, and a replaceable error.
#[fixture]
fn loaded_list() -> ThreadList {
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
        title: "Previous result".to_owned(),
        body: None,
        html_url: None,
        created_at: timestamp,
        updated_at: timestamp,
        closed_at: None,
        labels: Vec::new(),
        assignees: Vec::new(),
        provider_data: ProviderData::new(),
    };
    ThreadList {
        items: vec![ThreadSummary {
            repository,
            discussion,
            coverage: Vec::new(),
        }],
        selected: Some(0),
        offset: 20,
        next_offset: Some(40),
        generation: 2,
        loading: false,
        error: Some("previous failure".to_owned()),
    }
}

#[rstest]
fn beginning_a_read_clears_obsolete_navigation_and_error(mut loaded_list: ThreadList) {
    assert_eq!(loaded_list.begin(), 3);
    assert!(loaded_list.loading);
    assert!(loaded_list.items.is_empty());
    assert_eq!(loaded_list.selected, None);
    assert_eq!(loaded_list.next_offset, None);
    assert_eq!(loaded_list.error, None);
    assert_eq!(loaded_list.offset, 20);
}

#[rstest]
fn current_success_selects_the_first_result_and_uses_page_continuation(
    mut loaded_list: ThreadList,
) {
    let mut row = loaded_list.items[0].clone();
    row.discussion.title = "Current result".to_owned();
    let reply = ThreadReply {
        generation: 2,
        offset: 40,
        result: Ok(Box::new(ThreadPage {
            items: vec![row],
            next_offset: Some(60),
            coverage: Vec::new(),
        })),
    };
    assert_eq!(loaded_list.apply(reply), None);
    assert_eq!(loaded_list.items[0].discussion.title, "Current result");
    assert_eq!(loaded_list.selected, Some(0));
    assert_eq!(loaded_list.offset, 40);
    assert_eq!(loaded_list.next_offset, Some(60));
    assert_eq!(loaded_list.error, None);
    assert!(!loaded_list.loading);
}

#[rstest]
fn empty_page_has_no_selection_or_continuation(mut loaded_list: ThreadList) {
    let reply = ThreadReply {
        generation: 2,
        offset: 40,
        result: Ok(Box::new(ThreadPage {
            items: Vec::new(),
            next_offset: None,
            coverage: Vec::new(),
        })),
    };
    assert_eq!(loaded_list.apply(reply), None);
    assert!(loaded_list.items.is_empty());
    assert_eq!(loaded_list.selected, None);
    assert_eq!(loaded_list.next_offset, None);
    assert_eq!(loaded_list.offset, 40);
}

#[rstest]
fn failed_page_cannot_leave_obsolete_rows_available_for_navigation(mut loaded_list: ThreadList) {
    let reply = ThreadReply {
        generation: 2,
        offset: 40,
        result: Err("read failed".to_owned()),
    };
    assert_eq!(loaded_list.apply(reply).as_deref(), Some("read failed"));
    assert_eq!(loaded_list.error.as_deref(), Some("read failed"));
    assert!(loaded_list.items.is_empty());
    assert_eq!(loaded_list.selected, None);
    assert_eq!(loaded_list.next_offset, None);
    assert_eq!(loaded_list.offset, 40);
    assert!(!loaded_list.loading);
}

#[rstest]
fn stale_failure_keeps_the_newer_read_pending_at_its_current_offset(mut loaded_list: ThreadList) {
    assert_eq!(loaded_list.begin(), 3);
    let reply = ThreadReply {
        generation: 2,
        offset: 999,
        result: Err("stale failure".to_owned()),
    };
    assert_eq!(loaded_list.apply(reply), None);
    assert_eq!(loaded_list.generation, 3);
    assert!(loaded_list.loading);
    assert_eq!(loaded_list.offset, 20);
    assert_eq!(loaded_list.error, None);
}
