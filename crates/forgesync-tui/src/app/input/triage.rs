//! # Handle cluster triage input
//!
//! Triage input moves through cluster lists and detail, and triggers explicit local decisions
//! where the UI offers them. It uses the selected cluster identity from `App` rather than
//! reconstructing one from displayed text.
//!
//! The engine and store own the decision effect. This module owns the user's interaction path and
//! the state transition after an operation returns.

use crossterm::event::KeyCode;
use forgesync_core::identity::RunId;
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};

use crate::app::{App, move_index};
use crate::query::QueryAction;

impl App {
    /// Maps failure-view keys to run selection and retry actions.
    pub fn handle_failures_key(&mut self, code: KeyCode) -> Vec<QueryAction> {
        match code {
            KeyCode::Char('t') => self.retry_selected_run(),
            KeyCode::Up | KeyCode::Char('k') => self.move_failure_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_failure_selection(1),
            _ => Vec::new(),
        }
    }

    /// Requests retry only when the selected ledger row contains a valid durable run identity.
    fn retry_selected_run(&self) -> Vec<QueryAction> {
        self.failures
            .get(self.selected_failure)
            .and_then(|run| RunId::new(run.id).ok())
            .map(|run_id| vec![QueryAction::Retry(run_id)])
            .unwrap_or_default()
    }

    /// Keeps failure selection within the loaded ledger; empty lists remain unselected at zero.
    fn move_failure_selection(&mut self, direction: i8) -> Vec<QueryAction> {
        if direction > 0 && self.failures.is_empty() {
            return Vec::new();
        }
        let last = self.failures.len().saturating_sub(1);
        self.selected_failure = move_index(self.selected_failure, last, direction);
        Vec::new()
    }

    /// Maps cluster-list keys to selection and local decision actions.
    pub fn handle_clusters_key(&mut self, code: KeyCode) -> Vec<QueryAction> {
        match code {
            KeyCode::Char('d') => self.toggle_selected_cluster(),
            KeyCode::Up | KeyCode::Char('k') => self.move_cluster_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_cluster_selection(1),
            KeyCode::Enter => self.open_selected_cluster(),
            _ => Vec::new(),
        }
    }

    /// Toggles dismissal based on the currently loaded summary, without optimistic local mutation.
    fn toggle_selected_cluster(&self) -> Vec<QueryAction> {
        self.clusters
            .get(self.selected_cluster)
            .map(|cluster| {
                vec![QueryAction::DismissCluster {
                    id: cluster.id,
                    dismissed: !cluster.dismissed,
                }]
            })
            .unwrap_or_default()
    }

    /// Moves within the loaded cluster page without starting a detail query.
    fn move_cluster_selection(&mut self, direction: i8) -> Vec<QueryAction> {
        if direction > 0 && self.clusters.is_empty() {
            return Vec::new();
        }
        let last = self.clusters.len().saturating_sub(1);
        self.selected_cluster = move_index(self.selected_cluster, last, direction);
        Vec::new()
    }

    /// Starts a generation-tracked detail request for the selected cluster.
    fn open_selected_cluster(&mut self) -> Vec<QueryAction> {
        self.begin_cluster_detail()
            .map(|(generation, id)| vec![QueryAction::ClusterDetail { generation, id }])
            .unwrap_or_default()
    }

    /// Maps cluster-detail keys to member selection and decisions.
    pub fn handle_cluster_detail_key(&mut self, code: KeyCode) -> Vec<QueryAction> {
        match code {
            KeyCode::Char('d') => self.toggle_open_cluster(),
            KeyCode::Char('e' | 'i' | 'k') => self.cluster_member_action(code),
            KeyCode::Up => self.move_member_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_member_selection(1),
            _ => Vec::new(),
        }
    }

    /// Requests dismissal or restoration for the cluster whose detail is currently displayed.
    fn toggle_open_cluster(&self) -> Vec<QueryAction> {
        self.cluster_detail
            .as_ref()
            .map(|detail| {
                vec![QueryAction::DismissCluster {
                    id: detail.cluster.id,
                    dismissed: !detail.cluster.dismissed,
                }]
            })
            .unwrap_or_default()
    }

    /// Keeps the member cursor bounded by the current detail response, including an empty result.
    fn move_member_selection(&mut self, direction: i8) -> Vec<QueryAction> {
        if direction > 0
            && self
                .cluster_detail
                .as_ref()
                .is_none_or(|detail| detail.members.is_empty())
        {
            return Vec::new();
        }
        let last = self
            .cluster_detail
            .as_ref()
            .map_or(0, |detail| detail.members.len().saturating_sub(1));
        self.selected_cluster_member = move_index(self.selected_cluster_member, last, direction);
        Vec::new()
    }

    /// Builds a local maintainer action for the currently selected cluster member.
    fn cluster_member_action(&self, code: KeyCode) -> Vec<QueryAction> {
        let Some(detail) = &self.cluster_detail else {
            return Vec::new();
        };
        let Some(member) = detail.members.get(self.selected_cluster_member) else {
            return Vec::new();
        };
        let reference = ThreadSelector::new(
            RepositorySelector::from_repository(&member.summary.repository),
            member.summary.discussion.id.number(),
        );
        let action = match code {
            KeyCode::Char('e') => QueryAction::SetClusterMemberExcluded {
                id: detail.cluster.id,
                reference,
                excluded: true,
            },
            KeyCode::Char('i') => QueryAction::SetClusterMemberExcluded {
                id: detail.cluster.id,
                reference,
                excluded: false,
            },
            KeyCode::Char('k') => QueryAction::SetCanonicalClusterMember {
                id: detail.cluster.id,
                reference,
            },
            _ => return Vec::new(),
        };
        vec![action]
    }
}
