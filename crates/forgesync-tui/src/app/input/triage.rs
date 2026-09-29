//! Failure and cluster screen actions.

use super::{App, KeyCode, QueryAction, RepositorySelector};
use crate::app::{RunId, ThreadSelector, move_index};

impl App {
    pub(super) fn handle_failures_key(&mut self, code: KeyCode) -> Vec<QueryAction> {
        match code {
            KeyCode::Char('t') => self
                .failures
                .get(self.selected_failure)
                .and_then(|run| RunId::new(run.id).ok())
                .map(|run_id| vec![QueryAction::Retry(run_id)])
                .unwrap_or_default(),
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_failure = move_index(
                    self.selected_failure,
                    self.failures.len().saturating_sub(1),
                    -1,
                );
                Vec::new()
            }
            KeyCode::Down | KeyCode::Char('j') if !self.failures.is_empty() => {
                self.selected_failure = move_index(
                    self.selected_failure,
                    self.failures.len().saturating_sub(1),
                    1,
                );
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn handle_clusters_key(&mut self, code: KeyCode) -> Vec<QueryAction> {
        match code {
            KeyCode::Char('d') => self
                .clusters
                .get(self.selected_cluster)
                .map(|cluster| {
                    vec![QueryAction::DismissCluster {
                        id: cluster.id,
                        dismissed: !cluster.dismissed,
                    }]
                })
                .unwrap_or_default(),
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_cluster = move_index(
                    self.selected_cluster,
                    self.clusters.len().saturating_sub(1),
                    -1,
                );
                Vec::new()
            }
            KeyCode::Down | KeyCode::Char('j') if !self.clusters.is_empty() => {
                self.selected_cluster = move_index(
                    self.selected_cluster,
                    self.clusters.len().saturating_sub(1),
                    1,
                );
                Vec::new()
            }
            KeyCode::Enter => self
                .begin_cluster_detail()
                .map(|(generation, id)| vec![QueryAction::ClusterDetail { generation, id }])
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    pub(super) fn handle_cluster_detail_key(&mut self, code: KeyCode) -> Vec<QueryAction> {
        match code {
            KeyCode::Char('d') => self
                .cluster_detail
                .as_ref()
                .map(|detail| {
                    vec![QueryAction::DismissCluster {
                        id: detail.cluster.id,
                        dismissed: !detail.cluster.dismissed,
                    }]
                })
                .unwrap_or_default(),
            KeyCode::Char('e' | 'i' | 'k') => self.cluster_member_action(code),
            KeyCode::Up => {
                let last = self
                    .cluster_detail
                    .as_ref()
                    .map_or(0, |detail| detail.members.len().saturating_sub(1));
                self.selected_cluster_member = move_index(self.selected_cluster_member, last, -1);
                Vec::new()
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(detail) = &self.cluster_detail
                    && !detail.members.is_empty()
                {
                    self.selected_cluster_member = move_index(
                        self.selected_cluster_member,
                        detail.members.len().saturating_sub(1),
                        1,
                    );
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

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
