//! Background archive reads and the single writer.
//!
//! Dispatch begins the target panel's generation synchronously, before spawning, so a reply from
//! an earlier request is rejected by that panel. Results return as [`QueryMessage`] values.

use std::collections::HashMap;
use std::sync::Arc;

use forgesync_core::identity::{GitHubHost, RunId};
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use tokio::runtime::Handle;
use tokio::sync::mpsc::Sender;

use crate::app::App;
use crate::app::messages::QueryMessage;
use crate::query::tasks::QueryTasks;

pub mod failures;
mod operations;
mod progress;
mod reads;
pub mod tasks;

/// Work requested by terminal input.
#[derive(Debug, Eq, PartialEq)]
pub enum QueryAction {
    Read(Read),
    Operation(Operation),
    /// Cooperatively cancels the running writer, if any.
    CancelOperation,
}

/// A local archive read. An empty repository scope selects every registered repository.
#[derive(Debug, Eq, PartialEq)]
pub enum Read {
    Repositories,
    Threads {
        /// Keyword query; `None` browses by update time.
        query: Option<String>,
        repositories: Vec<RepositorySelector>,
        offset: u64,
    },
    Detail(ThreadSelector),
    Coverage,
    Failures,
    Clusters {
        repositories: Vec<RepositorySelector>,
    },
    ClusterDetail(u64),
}

/// A writer action. Sync, refresh, and retry contact GitHub; cluster decisions are local only.
#[derive(Debug, Eq, PartialEq)]
pub enum Operation {
    Sync {
        repositories: Vec<RepositorySelector>,
    },
    Refresh {
        repositories: Vec<RepositorySelector>,
    },
    Retry(RunId),
    DismissCluster {
        id: u64,
    },
    RestoreCluster {
        id: u64,
    },
    ExcludeClusterMember {
        id: u64,
        reference: ThreadSelector,
    },
    IncludeClusterMember {
        id: u64,
        reference: ThreadSelector,
    },
    SetCanonicalClusterMember {
        id: u64,
        reference: ThreadSelector,
    },
}

impl From<Read> for QueryAction {
    fn from(read: Read) -> Self {
        Self::Read(read)
    }
}

impl From<Operation> for QueryAction {
    fn from(operation: Operation) -> Self {
        Self::Operation(operation)
    }
}

/// Borrowed session resources for starting background work.
pub struct QueryDispatch<'a> {
    pub archive: &'a Arc<Archive>,
    pub clients: &'a Arc<HashMap<GitHubHost, GitHubClient>>,
    pub runtime: &'a Handle,
    pub sender: &'a Sender<QueryMessage>,
    pub tasks: &'a mut QueryTasks,
}

impl QueryDispatch<'_> {
    /// Starts one action without waiting for its result.
    pub fn start(&mut self, action: QueryAction, app: &mut App) {
        match action {
            QueryAction::Read(read) => self.start_read(read, app),
            QueryAction::Operation(operation) => self.start_operation(operation, app),
            QueryAction::CancelOperation => self.tasks.cancel_operation(),
        }
    }
}
