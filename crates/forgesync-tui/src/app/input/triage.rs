//! # Handle cluster triage input
//!
//! Triage input moves through cluster lists and detail, and triggers explicit local decisions
//! where the UI offers them. It uses the selected cluster identity from `App` rather than
//! reconstructing one from displayed text.
//!
//! The engine and store own the decision effect. This module owns the user's interaction path and
//! immediate selection transitions and the action submitted for asynchronous execution. Reply
//! handlers elsewhere in `app` apply results after checking their request generation.
//!
//! Failure input selects loaded ledger rows and requests retry by checked durable run identity.
//! Cluster-list input selects loaded summaries, opens details, and requests dismissal or restore
//! based on the displayed decision state. Cluster-detail input selects members and requests the
//! specific canonical/exclude/include decision offered by that key.
//!
//! Decision keys return typed actions without optimistically rewriting durable state. The displayed
//! summary can change before execution; engine/store operations enforce their own identity and
//! write fences. Empty selections return no action. Navigation stays within loaded rows and does
//! not infer identifiers from titles or rendered text.
//!
//! Shared screen switching, search editing, Escape, and quit behavior remain in the parent input
//! module. Rendering is separate from both submission and result application.

use crossterm::event::KeyCode;
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};

use crate::app::{App, move_index};
use crate::query::{Operation, QueryAction, Read};

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
        self.failure_list
            .runs
            .data
            .get(self.failure_list.selected)
            .map(|run| vec![QueryAction::Operation(Operation::Retry(run.id))])
            .unwrap_or_default()
    }

    /// Keeps failure selection within the loaded ledger; empty lists remain unselected at zero.
    fn move_failure_selection(&mut self, direction: i8) -> Vec<QueryAction> {
        if direction > 0 && self.failure_list.runs.data.is_empty() {
            return Vec::new();
        }
        let last = self.failure_list.runs.data.len().saturating_sub(1);
        self.failure_list.selected = move_index(self.failure_list.selected, last, direction);
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
        self.cluster_list
            .rows
            .data
            .get(self.cluster_list.selected)
            .map(|cluster| {
                let action = if cluster.dismissed {
                    QueryAction::Operation(Operation::RestoreCluster { id: cluster.id })
                } else {
                    QueryAction::Operation(Operation::DismissCluster { id: cluster.id })
                };
                vec![action]
            })
            .unwrap_or_default()
    }

    /// Moves within the loaded cluster page without starting a detail query.
    fn move_cluster_selection(&mut self, direction: i8) -> Vec<QueryAction> {
        if direction > 0 && self.cluster_list.rows.data.is_empty() {
            return Vec::new();
        }
        let last = self.cluster_list.rows.data.len().saturating_sub(1);
        self.cluster_list.selected = move_index(self.cluster_list.selected, last, direction);
        Vec::new()
    }

    fn open_selected_cluster(&mut self) -> Vec<QueryAction> {
        self.cluster_list
            .rows
            .data
            .get(self.cluster_list.selected)
            .map(|cluster| vec![QueryAction::Read(Read::ClusterDetail(cluster.id))])
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
        self.cluster_detail_pane
            .detail
            .data
            .as_ref()
            .map(|detail| {
                let action = if detail.cluster.dismissed {
                    QueryAction::Operation(Operation::RestoreCluster {
                        id: detail.cluster.id,
                    })
                } else {
                    QueryAction::Operation(Operation::DismissCluster {
                        id: detail.cluster.id,
                    })
                };
                vec![action]
            })
            .unwrap_or_default()
    }

    /// Keeps the member cursor bounded by the current detail response, including an empty result.
    fn move_member_selection(&mut self, direction: i8) -> Vec<QueryAction> {
        if direction > 0
            && self
                .cluster_detail_pane
                .detail
                .data
                .as_ref()
                .is_none_or(|detail| detail.members.is_empty())
        {
            return Vec::new();
        }
        let last = self
            .cluster_detail_pane
            .detail
            .data
            .as_ref()
            .map_or(0, |detail| detail.members.len().saturating_sub(1));
        self.cluster_detail_pane.selected_member =
            move_index(self.cluster_detail_pane.selected_member, last, direction);
        Vec::new()
    }

    /// Builds a local maintainer action for the currently selected cluster member.
    fn cluster_member_action(&self, code: KeyCode) -> Vec<QueryAction> {
        let Some(detail) = &self.cluster_detail_pane.detail.data else {
            return Vec::new();
        };
        let Some(member) = detail.members.get(self.cluster_detail_pane.selected_member) else {
            return Vec::new();
        };
        let reference = ThreadSelector::new(
            RepositorySelector::from_repository(&member.summary.repository),
            member.summary.discussion.id.number(),
        );
        let action = match code {
            KeyCode::Char('e') => QueryAction::Operation(Operation::ExcludeClusterMember {
                id: detail.cluster.id,
                reference,
            }),
            KeyCode::Char('i') => QueryAction::Operation(Operation::IncludeClusterMember {
                id: detail.cluster.id,
                reference,
            }),
            KeyCode::Char('k') => QueryAction::Operation(Operation::SetCanonicalClusterMember {
                id: detail.cluster.id,
                reference,
            }),
            _ => return Vec::new(),
        };
        vec![action]
    }
}

#[cfg(test)]
mod tests {
    //! Named maintainer requests preserve the loaded cluster and member target.
    //!
    //! These linear cases inspect returned actions before query execution. They ensure that
    //! dismissal restoration and member decisions use durable identities from the displayed
    //! detail instead of inferring a target from cursor text. Store/engine suites cover applying
    //! those decisions; this boundary proves input intent without optimistic durable mutation.

    use crossterm::event::KeyCode;

    use crate::app::App;
    use crate::app::loadable::Loadable;
    use crate::app::panels::ClusterDetailPane;
    use crate::app::test_data::sample_cluster_detail;
    use crate::query::{Operation, QueryAction};

    #[test]
    fn dismissing_an_already_dismissed_cluster_requests_restore() {
        let mut detail = sample_cluster_detail();
        detail.cluster.dismissed = true;
        let app = App {
            cluster_detail_pane: ClusterDetailPane {
                detail: Loadable::loaded(Some(detail)),
                ..Default::default()
            },
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
            cluster_detail_pane: ClusterDetailPane {
                detail: Loadable::loaded(Some(sample_cluster_detail())),
                ..Default::default()
            },
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
