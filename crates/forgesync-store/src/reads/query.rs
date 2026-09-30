//! # Filter and page archived threads
//!
//! `Archive` list methods build bounded SQL queries over repository scope, state, and sort
//! criteria. Query-builder helpers apply common discussion filters consistently across list and
//! search paths.
//!
//! Pagination and counts belong here so the engine receives stable `ThreadPage` values rather than
//! database cursors. The caller chooses filters; this module owns SQL parameter binding and row
//! conversion.
//!
//! [`Archive::query_threads`] reads aggregate coverage first, then the ordered discussion rows,
//! then per-thread coverage for the visible page. These reads do not share a snapshot: concurrent
//! writers may advance between them. Stable ordering means deterministic tie rules for the rows
//! observed, not a frozen result set across offset pages.
//!
//! [`ThreadQuery`] owns bound SQL construction and selects relevance only when an FTS expression
//! is present; ordinary relevance requests use updated-time ordering. One extra row acts as a
//! continuation sentinel and is removed before coverage hydration. A blank supplied expression
//! returns an empty page with aggregate coverage rather than executing invalid FTS syntax.
//!
//! Repository scope and discussion-filter helpers are shared with other local read paths. They
//! require the same `r`/`t` SQL aliases and bind external values rather than interpolating them.
//! Repository display-name lookup is separate from stable repository identity used for scope.
//! No read acquires a writer lease, refreshes providers, or changes completeness records.

use forgesync_core::content::{Repository, ThreadKind};
use forgesync_core::identity::{GitHubHost, RepositoryId};
use sqlx::{QueryBuilder, Row, Sqlite};

use super::{
    StoredThreadSummary, ThreadPage, ThreadQuery, ThreadSort, ThreadStateFilter, ThreadSummary,
};
use crate::archive::Archive;
use crate::coverage_projection::{coverage_for_kind, load_thread_coverage};
use crate::error::StoreError;

impl Archive {
    /// Returns registered repositories in stable host, owner, and name order.
    pub async fn list_repositories(&self) -> Result<Vec<Repository>, StoreError> {
        let rows = sqlx::query(
            "SELECT payload_json FROM repositories ORDER BY host, owner COLLATE NOCASE, name COLLATE NOCASE, provider_id",
        )
        .fetch_all(&self.reader)
        .await?;
        rows.into_iter()
            .map(|row| {
                let payload_json: String = row.try_get("payload_json")?;
                Ok(serde_json::from_str(&payload_json)?)
            })
            .collect()
    }

    /// Finds a repository by stored host and case-insensitive current owner/name.
    ///
    /// Returns `None` when the display path is absent. This resolves a current display selector,
    /// not historical rename aliases; stable scope uses the returned repository identity instead.
    /// Database and persisted JSON decoding failures are propagated.
    pub async fn find_repository(
        &self,
        host: &GitHubHost,
        owner: &str,
        name: &str,
    ) -> Result<Option<Repository>, StoreError> {
        let payload_json: Option<String> = sqlx::query_scalar(
            "SELECT payload_json FROM repositories WHERE host = ? AND owner COLLATE NOCASE = ? AND name COLLATE NOCASE = ?",
        )
        .bind(host.as_str())
        .bind(owner)
        .bind(name)
        .fetch_optional(&self.reader)
        .await?;
        payload_json
            .map(|payload| serde_json::from_str(&payload).map_err(StoreError::from))
            .transpose()
    }

    /// Returns a deterministically ordered local page without modifying durable archive state.
    ///
    /// Reads aggregate coverage, discussion rows, and item coverage separately. Concurrent writers
    /// can advance between those reads or between offset pages. A blank supplied FTS expression
    /// returns no items or continuation while retaining aggregate coverage. A nonblank expression
    /// is bound as FTS syntax; the engine owns literal-versus-advanced query interpretation.
    ///
    /// # Errors
    ///
    /// Propagates database, stored JSON, and integer conversion failures. FTS-related database
    /// failures are classified as [`StoreError::InvalidSearchQuery`] by the local error predicate.
    /// The nonzero query limit is trusted; upper page-size policy belongs to the engine.
    pub async fn query_threads(&self, query: &ThreadQuery) -> Result<ThreadPage, StoreError> {
        let coverage = self.coverage_summary(&query.repositories).await?;
        if query
            .match_expression
            .as_deref()
            .is_some_and(|expression| expression.trim().is_empty())
        {
            return Ok(ThreadPage {
                items: Vec::new(),
                next_offset: None,
                coverage,
            });
        }

        let mut statement = query.statement()?;
        let rows = statement
            .build()
            .fetch_all(&self.reader)
            .await
            .map_err(|error| {
                if query.match_expression.is_some() && is_fts_syntax_error(&error) {
                    StoreError::InvalidSearchQuery
                } else {
                    StoreError::Database(error)
                }
            })?;
        let stored_summaries = rows
            .into_iter()
            .map(StoredThreadSummary::decode)
            .collect::<Result<Vec<_>, _>>()?;
        self.thread_page(query, stored_summaries, coverage).await
    }

    /// Removes the pagination sentinel and hydrates applicable family coverage in one batch.
    async fn thread_page(
        &self,
        query: &ThreadQuery,
        mut stored_summaries: Vec<StoredThreadSummary>,
        coverage: Vec<super::FamilyCoverageSummary>,
    ) -> Result<ThreadPage, StoreError> {
        let has_more = i64::try_from(stored_summaries.len())
            .map_err(|_| StoreError::IntegerOutOfRange)?
            > i64::from(query.limit.get());
        if has_more {
            stored_summaries.pop();
        }
        let row_ids = stored_summaries
            .iter()
            .map(|stored| stored.row_id)
            .collect::<Vec<_>>();
        let coverage_by_thread = load_thread_coverage(&self.reader, &row_ids).await?;
        let items = stored_summaries
            .into_iter()
            .map(|mut stored| {
                let states = coverage_by_thread.get(&stored.row_id);
                stored.summary.coverage = coverage_for_kind(&stored.summary.discussion, states);
                stored.summary
            })
            .collect::<Vec<_>>();
        let next_offset = if has_more {
            query
                .offset
                .checked_add(u64::try_from(items.len()).map_err(|_| StoreError::IntegerOutOfRange)?)
                .ok_or(StoreError::IntegerOutOfRange)
                .map(Some)?
        } else {
            None
        };

        Ok(ThreadPage {
            items,
            next_offset,
            coverage,
        })
    }
}

impl ThreadQuery {
    /// Binds filters and deterministic ordering, including one extra row to detect a next page.
    fn statement(&self) -> Result<QueryBuilder<Sqlite>, StoreError> {
        let limit = i64::from(self.limit.get());
        let fetch_limit = limit.checked_add(1).ok_or(StoreError::IntegerOutOfRange)?;
        let offset = i64::try_from(self.offset).map_err(|_| StoreError::IntegerOutOfRange)?;
        let uses_fts = self.match_expression.is_some();
        let mut statement = QueryBuilder::<Sqlite>::new(if uses_fts {
            "SELECT t.id, r.payload_json AS repository_json, t.payload_json AS discussion_json FROM thread_search JOIN threads t ON t.id = thread_search.rowid JOIN repositories r ON r.id = t.repository_id WHERE thread_search MATCH "
        } else {
            "SELECT t.id, r.payload_json AS repository_json, t.payload_json AS discussion_json FROM threads t JOIN repositories r ON r.id = t.repository_id WHERE 1 = 1"
        });
        if let Some(expression) = self.match_expression.as_deref() {
            statement.push_bind(expression);
        }
        push_repository_scope(&mut statement, &self.repositories);
        push_discussion_filters(&mut statement, self.kind, self.state);
        if let Some(updated_since) = self.updated_since {
            statement
                .push(" AND t.updated_at_us >= ")
                .push_bind(updated_since.unix_microseconds());
        }
        statement
            .push(" ORDER BY ")
            .push(self.sort_order())
            .push(" LIMIT ")
            .push_bind(fetch_limit)
            .push(" OFFSET ")
            .push_bind(offset);

        Ok(statement)
    }

    /// Selects deterministic ordering for one discussion-list sort mode.
    fn sort_order(&self) -> &'static str {
        match (self.sort, self.match_expression.is_some()) {
            (ThreadSort::Relevance, true) => {
                "bm25(thread_search) ASC, t.updated_at_us DESC, r.full_name COLLATE NOCASE ASC, r.host ASC, t.number DESC, t.id ASC"
            }
            (ThreadSort::Created, _) => {
                "t.created_at_us DESC, r.full_name COLLATE NOCASE ASC, r.host ASC, t.number DESC, t.id ASC"
            }
            (ThreadSort::Updated, _) | (ThreadSort::Relevance, false) => {
                "t.updated_at_us DESC, r.full_name COLLATE NOCASE ASC, r.host ASC, t.number DESC, t.id ASC"
            }
        }
    }
}

impl StoredThreadSummary {
    /// Decodes persisted content before coverage is attached to the local projection.
    fn decode(row: sqlx::sqlite::SqliteRow) -> Result<Self, StoreError> {
        let row_id = row.try_get("id")?;
        let repository_json: String = row.try_get("repository_json")?;
        let discussion_json: String = row.try_get("discussion_json")?;
        Ok(Self {
            row_id,
            summary: ThreadSummary {
                repository: serde_json::from_str(&repository_json)?,
                discussion: serde_json::from_str(&discussion_json)?,
                coverage: Vec::new(),
            },
        })
    }
}

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

/// Separates invalid user FTS syntax from other SQLite failures.
fn is_fts_syntax_error(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|database| database.message().to_ascii_lowercase().contains("fts5"))
}
