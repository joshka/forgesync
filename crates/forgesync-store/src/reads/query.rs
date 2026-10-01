//! Filtered, paged thread lists and repository lookup.
//!
//! Aggregate coverage, page rows, and per-thread coverage are separate reads; deterministic tie
//! ordering does not freeze a result set across offset pages.

use forgesync_core::content::Repository;
use forgesync_core::identity::GitHubHost;
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::archive::Archive;
use crate::coverage_projection::{coverage_for_kind, load_thread_coverage};
use crate::error::StoreError;
use crate::reads::{StoredThreadSummary, ThreadPage, ThreadQuery, ThreadSort, ThreadSummary};
use crate::sql::{push_discussion_filters, push_repository_scope, to_sql_integer};

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

    /// Finds a repository by host and case-insensitive current owner/name (not rename aliases).
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

    /// Returns a deterministically ordered local page with aggregate scope coverage.
    ///
    /// A blank supplied FTS expression returns no items rather than executing invalid syntax;
    /// malformed FTS syntax returns [`StoreError::InvalidSearchQuery`].
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
        let has_more =
            stored_summaries.len() > usize::try_from(query.limit.get()).unwrap_or(usize::MAX);
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
        let offset = to_sql_integer(self.offset)?;
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

/// Separates invalid user FTS syntax from other SQLite failures.
fn is_fts_syntax_error(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|database| database.message().to_ascii_lowercase().contains("fts5"))
}
