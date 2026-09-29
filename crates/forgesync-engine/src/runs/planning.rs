//! # Resolve durable failures into deterministic retry requests
//!
//! `RetryPlanner` borrows one run detail and the caller's family restriction. It selects unresolved
//! failures, resolves each repository from durable identity or the matching job before parsing a
//! target, and combines work with the same repository and thread-state scope.
//!
//! `RecordedSelection` decodes the original run's scope once. A family-specific failure requests
//! only that child family; a failure without a family restores the original inclusion flags. A
//! failure's explicit open/closed/all key takes precedence over the original thread-state scope.
//!
//! This module performs no provider I/O and changes no ledger records. The returned `RetryPlan`
//! orders scopes and failure IDs deterministically. Missing or malformed recorded flags default
//! to false, while an unresolvable target and an empty selection remain typed engine errors.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_store::runs::{RunDetail, RunFailureRecord};

use super::{RetryPlan, RetryScope};
use crate::error::EngineError;
use crate::reference::RepositorySelector;
use crate::sync::SyncThreadScope;

/// Interprets a loaded ledger without contacting providers or mutating failure records.
pub fn plan(detail: &RunDetail, families: &[EvidenceFamily]) -> Result<RetryPlan, EngineError> {
    let planner = RetryPlanner {
        detail,
        families,
        original: RecordedSelection::from_scope(&detail.run.scope),
    };
    planner.plan()
}

/// Immutable ledger context and original request needed to resolve each selected failure.
struct RetryPlanner<'a> {
    /// Run, jobs, and failure records read together from the archive.
    detail: &'a RunDetail,
    /// Requested family restriction; empty includes failures with or without a family.
    families: &'a [EvidenceFamily],
    /// Original thread scope and child-family inclusion facts decoded from the run.
    original: RecordedSelection,
}

impl RetryPlanner<'_> {
    /// Collects selected failure identities and merges their minimal acquisition scopes.
    fn plan(&self) -> Result<RetryPlan, EngineError> {
        let mut plan = RetryPlan {
            parent_run_id: self.detail.run.id,
            failure_ids: Vec::new(),
            scopes: Vec::new(),
        };
        for failure in self
            .detail
            .failures
            .iter()
            .filter(|failure| self.includes(failure))
        {
            let repository = self.repository(failure)?;
            let scope = self.original.retry_scope(repository, failure);
            plan.merge_scope(scope);
            plan.failure_ids.push(failure.id);
        }
        plan.ordered()
    }

    /// Selects unresolved work, requiring an explicit family match when the caller restricts it.
    fn includes(&self, failure: &RunFailureRecord) -> bool {
        failure.resolved_at.is_none()
            && (self.families.is_empty()
                || failure
                    .family
                    .is_some_and(|family| self.families.contains(&family)))
    }

    /// Prefers durable identities over target parsing, preserving the original fallback order.
    fn repository(&self, failure: &RunFailureRecord) -> Result<RepositorySelector, EngineError> {
        let matching_job = self.detail.jobs.iter().find(|job| {
            job.repository.full_name == failure.target
                && failure.family.is_none_or(|family| job.family == family)
                && (failure.scope_key.is_empty() || job.scope_key == failure.scope_key)
        });
        failure
            .repository
            .as_ref()
            .or_else(|| matching_job.map(|job| &job.repository))
            .map(RepositorySelector::from_repository)
            .or_else(|| failure.target.parse::<RepositorySelector>().ok())
            .ok_or_else(|| EngineError::RetryTargetInvalid {
                target: failure.target.clone(),
            })
    }
}

/// Original selection facts used when a failed unit does not identify one evidence family.
#[derive(Debug, Eq, PartialEq)]
struct RecordedSelection {
    /// Default thread-state scope when the failure does not record open, closed, or all.
    scope: SyncThreadScope,
    /// Whether the original run requested comments.
    comments: bool,
    /// Whether the original run requested pull-request reviews.
    reviews: bool,
    /// Whether the original run requested review threads.
    review_threads: bool,
}

impl RecordedSelection {
    /// Decodes recorded request facts, treating absent or malformed inclusion flags as false.
    fn from_scope(scope: &serde_json::Value) -> Self {
        Self {
            scope: scope
                .get("thread_scope")
                .and_then(serde_json::Value::as_str)
                .and_then(thread_scope)
                .unwrap_or(SyncThreadScope::Default),
            comments: recorded_flag(scope, "include_comments"),
            reviews: recorded_flag(scope, "include_reviews"),
            review_threads: recorded_flag(scope, "include_review_threads"),
        }
    }

    /// Restores broad failures or selects only the failed child family for narrow failures.
    fn retry_scope(
        &self,
        repository: RepositorySelector,
        failure: &RunFailureRecord,
    ) -> RetryScope {
        let scope = thread_scope(&failure.scope_key).unwrap_or(self.scope);
        RetryScope {
            repository,
            scope,
            include_comments: failure
                .family
                .map_or(self.comments, |family| family == EvidenceFamily::Comments),
            include_reviews: failure
                .family
                .map_or(self.reviews, |family| family == EvidenceFamily::Reviews),
            include_review_threads: failure.family.map_or(self.review_threads, |family| {
                family == EvidenceFamily::ReviewThreads
            }),
        }
    }
}

/// Reads a recorded inclusion fact without interpreting malformed JSON as enabled work.
fn recorded_flag(scope: &serde_json::Value, name: &str) -> bool {
    scope
        .get(name)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

impl RetryPlan {
    /// Combines child-family requests only when both repository and thread-state scope agree.
    fn merge_scope(&mut self, scope: RetryScope) {
        if let Some(existing) = self.scopes.iter_mut().find(|existing| {
            existing.repository == scope.repository && existing.scope == scope.scope
        }) {
            existing.include_comments |= scope.include_comments;
            existing.include_reviews |= scope.include_reviews;
            existing.include_review_threads |= scope.include_review_threads;
        } else {
            self.scopes.push(scope);
        }
    }

    /// Rejects an empty selection and orders executable work independently of ledger row order.
    fn ordered(mut self) -> Result<Self, EngineError> {
        if self.scopes.is_empty() {
            return Err(EngineError::NoRetryableWork {
                id: self.parent_run_id.get(),
            });
        }
        self.scopes.sort_by(|left, right| {
            left.repository
                .as_url()
                .cmp(&right.repository.as_url())
                .then_with(|| scope_name(left.scope).cmp(scope_name(right.scope)))
        });
        self.failure_ids.sort_unstable();
        Ok(self)
    }
}

/// Recognizes explicit thread-state scopes; other keys retain the original request's default.
fn thread_scope(value: &str) -> Option<SyncThreadScope> {
    match value {
        "open" => Some(SyncThreadScope::Open),
        "closed" => Some(SyncThreadScope::Closed),
        "all" => Some(SyncThreadScope::All),
        _ => None,
    }
}

/// Provides the stable label used to order scopes within one repository.
fn scope_name(scope: SyncThreadScope) -> &'static str {
    match scope {
        SyncThreadScope::Default => "default",
        SyncThreadScope::Open => "open",
        SyncThreadScope::Closed => "closed",
        SyncThreadScope::All => "all",
    }
}

#[cfg(test)]
mod tests;
