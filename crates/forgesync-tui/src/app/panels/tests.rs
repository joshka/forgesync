use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId};
use forgesync_store::reads::ThreadDetail;
use ratatui::widgets::ListState;

use crate::app::panels::{
    ClusterDetailPane, ClusterList, DetailPane, RepositoryPicker, ThreadList,
};
use crate::app::test_data::{loaded_cluster_detail_pane, sample_cluster_detail, sample_repository};

fn selected_picker() -> RepositoryPicker {
    let mut picker = RepositoryPicker {
        state: ListState::default().with_selected(Some(1)),
        applied: Some(sample_repository()),
        ..RepositoryPicker::default()
    };
    picker.rows.data = vec![sample_repository()];
    picker
}

#[test]
fn inserted_repository_does_not_retarget_the_applied_scope() {
    let mut picker = selected_picker();
    let mut inserted = sample_repository();
    inserted.id = RepositoryId::new(
        GitHubHost::parse("github.com").expect("host"),
        ProviderId::new("42").expect("provider ID"),
    );
    inserted.full_name = "owner/inserted".to_owned();
    picker.rows.data.insert(0, inserted);

    picker.loaded();

    let applied = picker.applied.expect("applied repository");
    assert_eq!(applied.full_name, "owner/repo");
}

#[test]
fn empty_refresh_clamps_the_cursor_without_broadening_the_scope() {
    let mut picker = selected_picker();
    picker.rows.data.clear();

    picker.loaded();

    assert_eq!(picker.state.selected(), Some(0));
    assert_eq!(picker.applied, Some(sample_repository()));
}

#[test]
fn renamed_repository_updates_the_applied_scope_by_provider_identity() {
    let mut picker = selected_picker();
    picker.rows.data[0].full_name = "owner/renamed".to_owned();

    picker.loaded();

    let applied = picker.applied.expect("applied repository");
    assert_eq!(applied.full_name, "owner/renamed");
}

#[test]
fn beginning_a_thread_read_clears_old_rows_but_keeps_the_offset() {
    let mut list = ThreadList {
        state: ListState::default().with_selected(Some(0)),
        offset: 20,
        next_offset: Some(40),
        ..ThreadList::default()
    };
    list.rows.data = vec![sample_cluster_detail().members.remove(0).summary];

    list.begin();

    assert!(list.rows.data.is_empty());
    assert_eq!(list.state.selected(), None);
    assert_eq!(list.next_offset, None);
    assert_eq!(list.offset, 20);
}

#[test]
fn invalidated_detail_rejects_its_late_reply() {
    let mut pane = DetailPane::default();
    let mut status = None;
    let generation = pane.begin();
    pane.scroll = 42;

    pane.invalidate();

    let summary = sample_cluster_detail().members.remove(0).summary;
    let detail = ThreadDetail {
        summary,
        comments: Vec::new(),
        pull_request_metadata: Vec::new(),
        reviews: Vec::new(),
        review_threads: Vec::new(),
        timeline: Vec::new(),
    };
    assert!(
        !pane
            .detail
            .apply(generation, Ok(Some(Box::new(detail))), &mut status)
    );
    assert!(pane.detail.data.is_none());
    assert!(!pane.detail.loading);
    assert_eq!(pane.scroll, 0);
}

#[test]
fn opening_another_cluster_removes_old_members() {
    let mut pane = loaded_cluster_detail_pane(sample_cluster_detail());

    pane.begin(18);

    assert!(pane.detail.data.is_none());
    assert_eq!(pane.members.selected(), None);
}

#[test]
fn refreshing_the_same_cluster_keeps_its_members() {
    let mut pane = ClusterDetailPane::default();
    pane.detail.data = Some(sample_cluster_detail());

    pane.begin(17);

    let detail = pane.detail.data.expect("same-cluster cache");
    assert_eq!(detail.cluster.id, 17);
    assert!(pane.detail.loading);
}

#[test]
fn list_selection_stays_on_loaded_rows() {
    let mut list = ClusterList::default();
    list.select(ListState::select_next);
    assert_eq!(list.state.selected(), None);

    list.rows.data = vec![sample_cluster_detail().cluster; 2];
    list.loaded();
    assert_eq!(list.state.selected(), Some(0));

    list.select(ListState::select_last);
    assert_eq!(list.state.selected(), Some(1));
    list.select(ListState::select_next);
    assert_eq!(list.state.selected(), Some(1));

    list.rows.data.truncate(1);
    list.loaded();
    assert_eq!(list.selected().map(|cluster| cluster.id), Some(17));
}
