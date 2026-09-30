//! # Shared bound scope predicates for local reads
//!
//! [`push_repository_scope`] and [`push_discussion_filters`] append predicates to an existing
//! SQLite query builder. Discussion browsing, aggregate coverage, and embedding eligibility share
//! these fragments so identity and state filtering do not drift between their projections.
//!
//! Callers must provide an existing predicate before these `AND` fragments and use `r` for the
//! repository alias and `t` for the discussion alias. Empty repository scope adds no restriction;
//! source state `All` likewise adds none. Provider identity and kind values are bound parameters,
//! while known source-state literals are fixed SQL.
//!
//! This private module executes no statement and owns no ordering, pagination, date, FTS, or
//! coverage policy. It does not validate selected repositories or acquire a connection. The query
//! owner remains responsible for schema joins, transaction/snapshot scope, and row decoding.

use forgesync_core::content::ThreadKind;
use forgesync_core::identity::RepositoryId;
use sqlx::{QueryBuilder, Sqlite};

use crate::reads::ThreadStateFilter;

/// Adds bound repository IDs to a read query without string interpolation.
pub fn push_repository_scope(statement: &mut QueryBuilder<Sqlite>, repositories: &[RepositoryId]) {
    if repositories.is_empty() {
        return;
    }
    statement.push(" AND (");
    for (index, repository) in repositories.iter().enumerate() {
        if index > 0 {
            statement.push(" OR ");
        }
        statement
            .push("(r.host = ")
            .push_bind(repository.host().as_str())
            .push(" AND r.provider_id = ")
            .push_bind(repository.provider_id().as_str())
            .push(")");
    }
    statement.push(")");
}

/// Adds kind and state predicates to a query using the `t` discussion alias.
///
/// Kind values are bound, and the closed state choices use fixed SQL fragments. Date filtering
/// belongs to the owning query's construction rather than this shared helper.
pub fn push_discussion_filters(
    statement: &mut QueryBuilder<Sqlite>,
    kind: Option<ThreadKind>,
    state: ThreadStateFilter,
) {
    if let Some(kind) = kind {
        statement.push(" AND t.kind = ").push_bind(match kind {
            ThreadKind::Issue => "issue",
            ThreadKind::PullRequest => "pull_request",
        });
    }
    match state {
        ThreadStateFilter::All => {}
        ThreadStateFilter::Open => {
            statement.push(" AND t.state = 'open'");
        }
        ThreadStateFilter::Closed => {
            statement.push(" AND t.state = 'closed'");
        }
    }
}
