//! Applying background results and coordinating transitions that span panels.

use super::messages::QueryMessage;
use super::{App, RunningOperation};
use crate::query::{QueryAction, Read};

impl App {
    /// Reads issued when the browser opens.
    pub fn initial_actions(&self) -> [QueryAction; 2] {
        [Read::Repositories.into(), self.thread_action(None, 0)]
    }

    /// Reloads every view a finished writer may have changed.
    pub fn refresh_after_operation(&mut self) -> Vec<QueryAction> {
        let mut actions = vec![
            Read::Repositories.into(),
            self.thread_action(self.search_query.clone(), self.thread_list.offset),
            Read::Coverage.into(),
            Read::Failures.into(),
            Read::Clusters {
                repositories: self.repository_scope(),
            }
            .into(),
        ];
        if let Some(detail) = &self.cluster_detail_pane.detail.data {
            actions.push(Read::ClusterDetail(detail.cluster.id).into());
        }
        actions
    }

    /// Applies a result if it still belongs to its panel's current generation.
    pub fn apply(&mut self, message: QueryMessage) {
        let status = &mut self.status;
        match message {
            QueryMessage::Repositories { generation, result } => {
                let picker = &mut self.repository_picker;
                if picker.rows.apply(generation, result, status) {
                    picker.loaded();
                }
            }
            QueryMessage::Threads {
                generation,
                offset,
                result,
            } => {
                let list = &mut self.thread_list;
                let next_offset = result.as_ref().ok().and_then(|page| page.next_offset);
                if list
                    .rows
                    .apply(generation, result.map(|page| page.items), status)
                {
                    list.loaded(offset, next_offset);
                }
            }
            QueryMessage::Detail { generation, result } => {
                let pane = &mut self.detail_pane;
                if pane.detail.apply(generation, result.map(Some), status) {
                    pane.scroll = 0;
                }
            }
            QueryMessage::Coverage { generation, result } => {
                self.coverage
                    .apply(generation, result.map(|coverage| Some(*coverage)), status);
            }
            QueryMessage::Failures { generation, result } => {
                let list = &mut self.failure_list;
                if list.rows.apply(generation, result, status) {
                    list.loaded();
                }
            }
            QueryMessage::Clusters { generation, result } => {
                let list = &mut self.cluster_list;
                if list
                    .rows
                    .apply(generation, result.map(|page| page.items), status)
                {
                    list.loaded();
                }
            }
            QueryMessage::ClusterDetail { generation, result } => {
                let pane = &mut self.cluster_detail_pane;
                if pane
                    .detail
                    .apply(generation, result.map(|detail| Some(*detail)), status)
                {
                    pane.loaded();
                }
            }
            QueryMessage::OperationProgress(progress) => {
                if let Some(operation) = &mut self.operation {
                    operation.progress = Some(progress);
                }
            }
            QueryMessage::OperationFinished(result) => {
                self.operation = None;
                *status = Some(result.unwrap_or_else(|error| format!("Failed: {error}")));
            }
        }
    }

    /// Starts a thread read and invalidates detail, so a late reply for the old selection cannot
    /// reappear under the new scope.
    pub fn begin_threads(&mut self) -> u64 {
        self.detail_pane.invalidate();
        self.thread_list.begin()
    }

    /// Reserves the single writer slot; `false` while another operation runs.
    pub fn begin_operation(&mut self, label: &'static str) -> bool {
        if self.operation.is_some() {
            return false;
        }
        self.operation = Some(RunningOperation {
            label,
            progress: None,
            cancelling: false,
        });
        true
    }
}
