//! Resolve durable failures into deterministic retry requests.
//!
//! A family-specific failure requests only that child family; a failure without a family restores
//! the original inclusion flags. A failure's explicit open/closed/all key takes precedence over the
//! original thread-state scope.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_store::runs::{RunDetail, RunFailureRecord};

use super::{RetryPlan, RetryScope};
use crate::error::EngineError;
use crate::reference::RepositorySelector;
use crate::sync::{RunScope, SyncThreadScope};

/// Selects unresolved failures (restricted to `families` when nonempty) and merges their scopes.
pub fn plan(detail: &RunDetail, families: &[EvidenceFamily]) -> Result<RetryPlan, EngineError> {
    let original = RunScope::decode(&detail.run.scope);
    let mut plan = RetryPlan {
        parent_run_id: detail.run.id,
        failure_ids: Vec::new(),
        scopes: Vec::new(),
    };
    let selected = detail.failures.iter().filter(|failure| {
        failure.resolved_at.is_none()
            && (families.is_empty()
                || failure
                    .family
                    .is_some_and(|family| families.contains(&family)))
    });
    for failure in selected {
        let repository = failed_repository(detail, failure)?;
        plan.merge_scope(retry_scope(&original, repository, failure));
        plan.failure_ids.push(failure.id);
    }
    plan.ordered()
}

/// Prefers durable identities (the failure's, then a matching job's) over target parsing.
fn failed_repository(
    detail: &RunDetail,
    failure: &RunFailureRecord,
) -> Result<RepositorySelector, EngineError> {
    let matching_job = detail.jobs.iter().find(|job| {
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

/// Restores broad failures or selects only the failed child family for narrow failures.
fn retry_scope(
    original: &RunScope,
    repository: RepositorySelector,
    failure: &RunFailureRecord,
) -> RetryScope {
    let only = |family| failure.family.map(|failed| failed == family);
    RetryScope {
        repository,
        scope: thread_scope(&failure.scope_key).unwrap_or(original.thread_scope),
        include_comments: only(EvidenceFamily::Comments).unwrap_or(original.include_comments),
        include_reviews: only(EvidenceFamily::Reviews).unwrap_or(original.include_reviews),
        include_review_threads: only(EvidenceFamily::ReviewThreads)
            .unwrap_or(original.include_review_threads),
    }
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
