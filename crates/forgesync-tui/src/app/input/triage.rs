//! Failure and cluster triage keys. Decisions are submitted as operations; the displayed state
//! changes only when the archive is re-read after the writer finishes.

use crossterm::event::KeyCode;
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};
use ratatui::widgets::ListState;

use crate::app::App;
use crate::query::{Operation, QueryAction, Read};

impl App {
    pub fn handle_failures_key(&mut self, code: KeyCode) -> Vec<QueryAction> {
        match code {
            KeyCode::Char('t') => self.retry_selected_run(),
            KeyCode::Up | KeyCode::Char('k') => {
                self.failure_list.select(ListState::select_previous);
                Vec::new()
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.failure_list.select(ListState::select_next);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn retry_selected_run(&self) -> Vec<QueryAction> {
        self.failure_list
            .selected()
            .map(|run| vec![Operation::Retry(run.id).into()])
            .unwrap_or_default()
    }

    pub fn handle_clusters_key(&mut self, code: KeyCode) -> Vec<QueryAction> {
        match code {
            KeyCode::Char('d') => self.toggle_selected_cluster(),
            KeyCode::Up | KeyCode::Char('k') => {
                self.cluster_list.select(ListState::select_previous);
                Vec::new()
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.cluster_list.select(ListState::select_next);
                Vec::new()
            }
            KeyCode::Enter => self.open_selected_cluster(),
            _ => Vec::new(),
        }
    }

    fn toggle_selected_cluster(&self) -> Vec<QueryAction> {
        self.cluster_list
            .selected()
            .map(|cluster| vec![dismissal_toggle(cluster.id, cluster.dismissed)])
            .unwrap_or_default()
    }

    fn open_selected_cluster(&mut self) -> Vec<QueryAction> {
        self.cluster_list
            .selected()
            .map(|cluster| vec![Read::ClusterDetail(cluster.id).into()])
            .unwrap_or_default()
    }

    pub fn handle_cluster_detail_key(&mut self, code: KeyCode) -> Vec<QueryAction> {
        match code {
            KeyCode::Char('d') => self.toggle_open_cluster(),
            KeyCode::Char('e' | 'i' | 'k') => self.cluster_member_action(code),
            KeyCode::Up => {
                self.cluster_detail_pane
                    .select_member(ListState::select_previous);
                Vec::new()
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.cluster_detail_pane
                    .select_member(ListState::select_next);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn toggle_open_cluster(&self) -> Vec<QueryAction> {
        self.cluster_detail_pane
            .detail
            .data
            .as_ref()
            .map(|detail| {
                vec![dismissal_toggle(
                    detail.cluster.id,
                    detail.cluster.dismissed,
                )]
            })
            .unwrap_or_default()
    }

    fn cluster_member_action(&self, code: KeyCode) -> Vec<QueryAction> {
        let pane = &self.cluster_detail_pane;
        let Some(detail) = &pane.detail.data else {
            return Vec::new();
        };
        let Some(member) = pane
            .members
            .selected()
            .and_then(|index| detail.members.get(index))
        else {
            return Vec::new();
        };
        let id = detail.cluster.id;
        let reference = ThreadSelector::new(
            RepositorySelector::from_repository(&member.summary.repository),
            member.summary.discussion.id.number(),
        );
        let operation = match code {
            KeyCode::Char('e') => Operation::ExcludeClusterMember { id, reference },
            KeyCode::Char('i') => Operation::IncludeClusterMember { id, reference },
            KeyCode::Char('k') => Operation::SetCanonicalClusterMember { id, reference },
            _ => return Vec::new(),
        };
        vec![operation.into()]
    }
}

fn dismissal_toggle(id: u64, dismissed: bool) -> QueryAction {
    if dismissed {
        Operation::RestoreCluster { id }.into()
    } else {
        Operation::DismissCluster { id }.into()
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use crate::app::App;
    use crate::app::test_data::{loaded_cluster_detail_pane, sample_cluster_detail};
    use crate::query::{Operation, QueryAction};

    #[test]
    fn dismissing_an_already_dismissed_cluster_requests_restore() {
        let mut detail = sample_cluster_detail();
        detail.cluster.dismissed = true;
        let app = App {
            cluster_detail_pane: loaded_cluster_detail_pane(detail),
            ..Default::default()
        };

        let actions = app.toggle_open_cluster();

        assert_eq!(
            actions,
            vec![QueryAction::Operation(Operation::RestoreCluster { id: 17 })]
        );
    }

    #[test]
    fn include_key_requests_inclusion_of_the_loaded_member() {
        let app = App {
            cluster_detail_pane: loaded_cluster_detail_pane(sample_cluster_detail()),
            ..Default::default()
        };

        let actions = app.cluster_member_action(KeyCode::Char('i'));

        let reference = "owner/repo#7".parse().expect("thread selector");
        assert_eq!(
            actions,
            vec![QueryAction::Operation(Operation::IncludeClusterMember {
                id: 17,
                reference
            })]
        );
    }
}
