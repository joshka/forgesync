//! # Filter and page archived threads
//!
//! `Archive` list methods build bounded SQL queries over repository scope, state, and sort
//! criteria. Query-builder helpers apply common discussion filters consistently across list and
//! search paths.
//!
//! Pagination and counts belong here so the engine receives stable `ThreadPage` values rather than
//! database cursors. The caller chooses filters; this module owns SQL parameter binding and row
//! conversion.

use sqlx::Row;

use super::{
    Archive, GitHubHost, QueryBuilder, Repository, RepositoryId, Sqlite, StoreError,
    StoredThreadSummary, ThreadKind, ThreadPage, ThreadQuery, ThreadSort, ThreadStateFilter,
    ThreadSummary, coverage_for_kind, load_thread_coverage,
};

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

    /// Finds a repository by its current host, owner, and name.
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

    /// Returns a stable page of local discussions without modifying the archive.
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

        let limit = i64::from(query.limit.get());
        let fetch_limit = limit.checked_add(1).ok_or(StoreError::IntegerOutOfRange)?;
        let offset = i64::try_from(query.offset).map_err(|_| StoreError::IntegerOutOfRange)?;
        let uses_fts = query.match_expression.is_some();
        let mut statement = QueryBuilder::<Sqlite>::new(if uses_fts {
            "SELECT t.id, r.payload_json AS repository_json, t.payload_json AS discussion_json FROM thread_search JOIN threads t ON t.id = thread_search.rowid JOIN repositories r ON r.id = t.repository_id WHERE thread_search MATCH "
        } else {
            "SELECT t.id, r.payload_json AS repository_json, t.payload_json AS discussion_json FROM threads t JOIN repositories r ON r.id = t.repository_id WHERE 1 = 1"
        });
        if let Some(expression) = query.match_expression.as_deref() {
            statement.push_bind(expression);
        }
        push_repository_scope(&mut statement, &query.repositories);
        push_discussion_filters(&mut statement, query.kind, query.state);
        if let Some(updated_since) = query.updated_since {
            statement
                .push(" AND t.updated_at_us >= ")
                .push_bind(updated_since.unix_microseconds());
        }
        statement
            .push(" ORDER BY ")
            .push(sort_order(query.sort, uses_fts))
            .push(" LIMIT ")
            .push_bind(fetch_limit)
            .push(" OFFSET ")
            .push_bind(offset);

        let rows = statement
            .build()
            .fetch_all(&self.reader)
            .await
            .map_err(|error| {
                if uses_fts && is_fts_syntax_error(&error) {
                    StoreError::InvalidSearchQuery
                } else {
                    StoreError::Database(error)
                }
            })?;
        let mut stored_summaries = Vec::with_capacity(rows.len());
        for row in rows {
            let row_id: i64 = row.try_get("id")?;
            let repository_json: String = row.try_get("repository_json")?;
            let discussion_json: String = row.try_get("discussion_json")?;
            let repository = serde_json::from_str(&repository_json)?;
            let discussion = serde_json::from_str(&discussion_json)?;
            stored_summaries.push(StoredThreadSummary {
                row_id,
                summary: ThreadSummary {
                    repository,
                    discussion,
                    coverage: Vec::new(),
                },
            });
        }

        let has_more = i64::try_from(stored_summaries.len())
            .map_err(|_| StoreError::IntegerOutOfRange)?
            > limit;
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

/// Adds state, kind, and date predicates to a local discussion query.
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

/// Selects deterministic ordering for one discussion-list sort mode.
fn sort_order(sort: ThreadSort, uses_fts: bool) -> &'static str {
    match (sort, uses_fts) {
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

/// Separates invalid user FTS syntax from other SQLite failures.
fn is_fts_syntax_error(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|database| database.message().to_ascii_lowercase().contains("fts5"))
}
