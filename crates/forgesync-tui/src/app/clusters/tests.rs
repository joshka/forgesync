//! # Cluster cache ownership and safe member targeting
//!
//! These scenarios distinguish refreshing the same cluster from opening a different cluster.
//! Keeping a same-cluster cache supports responsive inspection; keeping another cluster's members
//! would allow keyboard decisions to use an unrelated target during loading.
//!
//! Fixed domain data comes from the app fixture module. Tests change the selected cluster or read
//! generation explicitly, then inspect both pane state and the resulting member-action boundary.
//! Provider acquisition and persisted triage decisions remain covered by engine/store workflows.

use crossterm::event::KeyCode;

use super::ClusterDetailPane;
use crate::app::test_data::sample_cluster_detail;
use crate::app::{App, Screen};

#[test]
fn opening_another_cluster_removes_old_members_before_keyboard_decisions() {
    let mut pane = ClusterDetailPane {
        data: Some(sample_cluster_detail()),
        ..Default::default()
    };
    assert_eq!(pane.begin(18), 1);
    assert!(pane.data.is_none());
    assert_eq!(pane.selected_member, 0);
    let mut app = App {
        screen: Screen::ClusterDetail,
        cluster_detail_pane: pane,
        ..App::default()
    };
    let actions = app.handle_cluster_detail_key(KeyCode::Char('x'));
    assert!(actions.is_empty());
}

#[test]
fn refreshing_the_same_cluster_retains_its_cache_and_clears_the_previous_error() {
    let mut pane = ClusterDetailPane {
        data: Some(sample_cluster_detail()),
        error: Some("previous failure".to_owned()),
        ..Default::default()
    };
    assert_eq!(pane.begin(17), 1);
    assert_eq!(
        pane.data.as_ref().expect("same-cluster cache").cluster.id,
        17
    );
    assert_eq!(pane.error, None);
    assert!(pane.loading);
}

#[test]
fn late_success_cannot_restore_the_previous_cluster_after_selection_changes() {
    let previous = sample_cluster_detail();
    let mut pane = ClusterDetailPane::default();
    assert_eq!(pane.begin(17), 1);
    assert_eq!(pane.apply(1, Ok(Box::new(previous.clone()))), None);
    assert_eq!(pane.begin(18), 2);
    assert_eq!(pane.apply(1, Ok(Box::new(previous))), None);
    assert!(pane.data.is_none());
    assert_eq!(pane.generation, 2);
    assert!(pane.loading);
}

#[test]
fn failed_refresh_of_the_same_cluster_retains_only_its_current_cache() {
    let mut pane = ClusterDetailPane {
        data: Some(sample_cluster_detail()),
        ..Default::default()
    };
    assert_eq!(pane.begin(17), 1);
    assert_eq!(
        pane.apply(1, Err("read failed".to_owned())).as_deref(),
        Some("read failed")
    );
    assert_eq!(pane.error.as_deref(), Some("read failed"));
    assert_eq!(
        pane.data.as_ref().expect("same-cluster cache").cluster.id,
        17
    );
    assert!(!pane.loading);
}
