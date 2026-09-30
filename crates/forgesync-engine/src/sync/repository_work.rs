//! # Scope and services for one repository's selected evidence work
//!
//! `RepositoryWork` binds a repository and thread-state scope to the archive, provider client,
//! and run ownership used by its comment and pull-request jobs. Those inputs travel together
//! throughout acquisition; callers construct the scope once rather than forwarding separate
//! repository, unit, lease context, and client arguments to each family operation.
//!
//! The type carries immutable execution scope. Comment and pull-request modules implement their
//! corresponding methods beside the acquisition logic. Mutable counters and terminal job state
//! remain with the job owners, so this value is not a second run-wide state container.

use forgesync_core::content::Repository;
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;

use crate::sync::scope::{ScopeUnit, SyncRunContext};

/// Immutable repository-family scope and the services authorized to acquire it.
#[derive(Clone, Copy)]
pub struct RepositoryWork<'a> {
    /// Archive already opened by the application for this run.
    pub archive: &'a Archive,
    /// Provider adapter for the repository's host.
    pub client: &'a GitHubClient,
    /// Normalized repository whose selected threads are visited.
    pub repository: &'a Repository,
    /// Thread-state selection and failure-ledger scope key.
    pub unit: ScopeUnit,
    /// Run ID, writer lease, cancellation, and progress channel.
    pub context: &'a SyncRunContext<'a>,
}
