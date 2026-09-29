use std::collections::HashMap;
use std::num::NonZeroU32;

use forgesync_core::content::{
    Comment, Discussion, PullRequestMetadata, Repository, Review, ReviewThread, ThreadKind,
};
use forgesync_core::coverage::{Coverage, CoverageState, EvidenceFamily};
use forgesync_core::identity::{
    CommitSha, GitHubHost, RepositoryId, ReviewThreadId, ThreadId, ThreadReference,
};
use forgesync_core::timestamp::UtcTimestamp;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::{Archive, ArchiveDiagnostics, ArchiveInfo, StagedItem, StoreError};

/// Source-state filter for a local discussion query.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThreadStateFilter {
    /// Include open, closed, and unrecognized source states.
    #[default]
    All,
    /// Include only discussions whose source state is open.
    Open,
    /// Include only discussions whose source state is closed.
    Closed,
}

/// Sort order for local discussion queries.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThreadSort {
    /// Rank full-text matches first; without a query, use update order.
    #[default]
    Relevance,
    /// Sort by source update time, newest first.
    Updated,
    /// Sort by source creation time, newest first.
    Created,
}

/// Typed filters and pagination for a local thread search or list.
#[derive(Clone, Debug)]
pub struct ThreadQuery {
    /// Resolved repository identities; an empty list includes every repository.
    pub repositories: Vec<RepositoryId>,
    /// Optional issue or pull-request filter.
    pub kind: Option<ThreadKind>,
    /// Source open/closed filter.
    pub state: ThreadStateFilter,
    /// Prepared FTS5 expression. `None` lists without text search.
    pub match_expression: Option<String>,
    /// Optional inclusive lower bound on the current source update timestamp.
    pub updated_since: Option<UtcTimestamp>,
    /// Sort policy, applied with deterministic ties.
    pub sort: ThreadSort,
    /// Maximum number of rows to return.
    pub limit: NonZeroU32,
    /// Number of matching rows to skip.
    pub offset: u64,
}

/// One discussion and its containing repository with current family coverage.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ThreadSummary {
    /// Current repository identity and display metadata.
    pub repository: Repository,
    /// Current normalized source discussion.
    pub discussion: Discussion,
    /// Current coverage for applicable evidence families.
    pub coverage: Vec<Coverage>,
}

/// A page of local discussion results and archive coverage for the repository scope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ThreadPage {
    /// Results in stable query order.
    pub items: Vec<ThreadSummary>,
    /// Offset to pass to the next request, when more results are available.
    pub next_offset: Option<u64>,
    /// Coverage totals for the selected repositories, even when no rows match.
    pub coverage: Vec<FamilyCoverageSummary>,
}

/// Current discussion content and selected family evidence for a thread reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ThreadDetail {
    /// Discussion, repository, and per-family coverage.
    pub summary: ThreadSummary,
    /// Current comments, when comment coverage has been acquired.
    pub comments: Vec<StagedItem<Comment>>,
    /// Current pull-request metadata, when acquired.
    pub pull_request_metadata: Vec<StagedItem<PullRequestMetadata>>,
    /// Current submitted reviews, when acquired.
    pub reviews: Vec<StagedItem<Review>>,
    /// Current review threads, when acquired.
    pub review_threads: Vec<StagedItem<ReviewThread>>,
    /// Chronological projection of current source content, not full revision history.
    pub timeline: Vec<ThreadTimelineEntry>,
}

/// One current source event ordered by its source timestamp when available.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ThreadTimelineEntry {
    /// Source timestamp; missing review timestamps sort after known events.
    pub occurred_at: Option<UtcTimestamp>,
    /// Typed event with the selected content and review context.
    pub event: ThreadTimelineEvent,
}

/// Event represented in the current thread timeline.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ThreadTimelineEvent {
    /// Original source creation event for the current discussion.
    ThreadCreated {
        /// Stable thread identity.
        thread: ThreadId,
        /// Current title.
        title: String,
    },
    /// Source closure time retained on the current discussion.
    ThreadClosed {
        /// Stable thread identity.
        thread: ThreadId,
    },
    /// Current issue or discussion comment.
    Comment {
        /// Normalized comment content and identity.
        comment: Comment,
    },
    /// Current submitted review.
    Review {
        /// Normalized review content and identity.
        review: Review,
    },
    /// A current review-thread state, which has no reliable independent event time.
    ReviewThread {
        /// Stable review-thread identity.
        id: ReviewThreadId,
        /// Pull-request head against which resolution was read.
        head_sha: CommitSha,
        /// Whether the source currently reports the thread resolved.
        is_resolved: bool,
        /// Whether the source currently reports the line outdated.
        is_outdated: bool,
        /// Current reviewed path, when available.
        path: Option<String>,
    },
    /// A current comment nested in a review thread.
    ReviewThreadComment {
        /// Stable parent review-thread identity.
        review_thread_id: ReviewThreadId,
        /// Pull-request head against which the parent state was read.
        head_sha: CommitSha,
        /// Current resolution state of the parent review thread.
        is_resolved: bool,
        /// Current outdated state of the parent review thread.
        is_outdated: bool,
        /// Current reviewed path, when available.
        path: Option<String>,
        /// Normalized nested comment.
        comment: Comment,
    },
}

/// Complete, incomplete, and missing counts for one applicable evidence family.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FamilyCoverageSummary {
    /// Family whose current coverage is counted.
    pub family: EvidenceFamily,
    /// Number of source threads to which this family applies.
    pub applicable_threads: u64,
    /// Applicable threads without an observation.
    pub missing: u64,
    /// Applicable threads with an incomplete latest collection.
    pub incomplete: u64,
    /// Applicable threads with a complete latest collection.
    pub complete: u64,
}

/// Archive metadata, local content counts, and family coverage totals.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ArchiveStatus {
    /// Validated lifecycle metadata.
    pub archive: ArchiveInfo,
    /// Number of registered repositories.
    pub repositories: u64,
    /// Number of archived issue and pull-request discussions.
    pub threads: u64,
    /// Number of archived issues.
    pub issues: u64,
    /// Number of archived pull requests.
    pub pull_requests: u64,
    /// Coverage totals across all applicable threads.
    pub coverage: Vec<FamilyCoverageSummary>,
    /// Schema, lease, and durable-work diagnostics.
    pub diagnostics: ArchiveDiagnostics,
}

struct StoredThreadSummary {
    row_id: i64,
    summary: ThreadSummary,
}

pub(crate) struct StoredCoverage {
    state: CoverageState,
    source_clock_state: String,
    source_clock_us: Option<i64>,
    snapshot_head_sha: Option<String>,
    current_head_sha: Option<String>,
}

const ALL_FAMILIES: [EvidenceFamily; 5] = [
    EvidenceFamily::Threads,
    EvidenceFamily::Comments,
    EvidenceFamily::PullRequestMetadata,
    EvidenceFamily::Reviews,
    EvidenceFamily::ReviewThreads,
];

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

    /// Returns current thread details and typed selected evidence for a resolved reference.
    pub async fn thread_detail(
        &self,
        reference: &ThreadReference,
    ) -> Result<ThreadDetail, StoreError> {
        let row = sqlx::query(
            "SELECT t.id, r.payload_json AS repository_json, t.payload_json AS discussion_json FROM threads t JOIN repositories r ON r.id = t.repository_id WHERE r.host = ? AND r.provider_id = ? AND t.number = ?",
        )
        .bind(reference.repository().host().as_str())
        .bind(reference.repository().provider_id().as_str())
        .bind(i64::try_from(reference.number().get()).map_err(|_| StoreError::IntegerOutOfRange)?)
        .fetch_optional(&self.reader)
        .await?
        .ok_or(StoreError::ThreadMissing)?;
        let row_id: i64 = row.try_get("id")?;
        let repository_json: String = row.try_get("repository_json")?;
        let discussion_json: String = row.try_get("discussion_json")?;
        let repository = serde_json::from_str(&repository_json)?;
        let discussion: Discussion = serde_json::from_str(&discussion_json)?;
        let coverage_by_thread = load_thread_coverage(&self.reader, &[row_id]).await?;
        let summary = ThreadSummary {
            repository,
            coverage: coverage_for_kind(&discussion, coverage_by_thread.get(&row_id)),
            discussion,
        };

        let comments: Vec<StagedItem<Comment>> =
            load_family_members(&self.reader, row_id, "comments").await?;
        let pull_request_metadata =
            load_family_members(&self.reader, row_id, "pull_request_metadata").await?;
        let reviews: Vec<StagedItem<Review>> =
            load_family_members(&self.reader, row_id, "reviews").await?;
        let review_threads: Vec<StagedItem<ReviewThread>> =
            load_family_members(&self.reader, row_id, "review_threads").await?;
        let timeline =
            build_thread_timeline(&summary.discussion, &comments, &reviews, &review_threads);

        Ok(ThreadDetail {
            summary,
            comments,
            pull_request_metadata,
            reviews,
            review_threads,
            timeline,
        })
    }

    /// Returns coverage counts for all families, optionally limited to resolved repositories.
    pub async fn coverage_summary(
        &self,
        repositories: &[RepositoryId],
    ) -> Result<Vec<FamilyCoverageSummary>, StoreError> {
        let mut summaries = Vec::with_capacity(ALL_FAMILIES.len());
        for family in ALL_FAMILIES {
            let mut statement = QueryBuilder::<Sqlite>::new(
                "SELECT COALESCE(c.status, 'missing') AS status, COUNT(*) AS item_count FROM threads t JOIN repositories r ON r.id = t.repository_id LEFT JOIN family_coverage c ON c.thread_id = t.id AND c.family = ",
            );
            statement
                .push_bind(evidence_family_name(family))
                .push(" WHERE 1 = 1");
            push_repository_scope(&mut statement, repositories);
            if is_pull_request_family(family) {
                statement.push(" AND t.kind = 'pull_request'");
            }
            statement.push(" GROUP BY COALESCE(c.status, 'missing')");
            let rows = statement.build().fetch_all(&self.reader).await?;
            let mut summary = FamilyCoverageSummary {
                family,
                applicable_threads: 0,
                missing: 0,
                incomplete: 0,
                complete: 0,
            };
            for row in rows {
                let status: String = row.try_get("status")?;
                let count: i64 = row.try_get("item_count")?;
                let count = u64::try_from(count).map_err(|_| StoreError::InvalidStoredCount)?;
                summary.applicable_threads = summary
                    .applicable_threads
                    .checked_add(count)
                    .ok_or(StoreError::IntegerOutOfRange)?;
                match status.as_str() {
                    "missing" => summary.missing = count,
                    "incomplete" => summary.incomplete = count,
                    "complete" => summary.complete = count,
                    _ => return Err(StoreError::InvalidStoredCoverage),
                }
            }
            summaries.push(summary);
        }
        Ok(summaries)
    }

    /// Returns local archive counts and aggregate per-family coverage.
    pub async fn archive_status(&self) -> Result<ArchiveStatus, StoreError> {
        let repositories: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM repositories")
            .fetch_one(&self.reader)
            .await?;
        let rows = sqlx::query("SELECT kind, COUNT(*) AS item_count FROM threads GROUP BY kind")
            .fetch_all(&self.reader)
            .await?;
        let mut issues = 0_u64;
        let mut pull_requests = 0_u64;
        for row in rows {
            let kind: String = row.try_get("kind")?;
            let count: i64 = row.try_get("item_count")?;
            let count = u64::try_from(count).map_err(|_| StoreError::InvalidStoredCount)?;
            match kind.as_str() {
                "issue" => issues = count,
                "pull_request" => pull_requests = count,
                _ => return Err(StoreError::InvalidStoredThreadKind(kind)),
            }
        }
        let repositories =
            u64::try_from(repositories).map_err(|_| StoreError::InvalidStoredCount)?;
        let threads = issues
            .checked_add(pull_requests)
            .ok_or(StoreError::IntegerOutOfRange)?;
        Ok(ArchiveStatus {
            archive: self.info().clone(),
            repositories,
            threads,
            issues,
            pull_requests,
            coverage: self.coverage_summary(&[]).await?,
            diagnostics: self.diagnostics().await?,
        })
    }
}

pub(crate) fn push_repository_scope(
    statement: &mut QueryBuilder<Sqlite>,
    repositories: &[RepositoryId],
) {
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

pub(crate) fn push_discussion_filters(
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

pub(crate) async fn load_thread_coverage(
    pool: &sqlx::SqlitePool,
    thread_ids: &[i64],
) -> Result<HashMap<i64, HashMap<EvidenceFamily, StoredCoverage>>, StoreError> {
    if thread_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut metadata_statement = QueryBuilder::<Sqlite>::new(
        "SELECT thread_id, payload_json FROM thread_family_membership WHERE family = 'pull_request_metadata' AND thread_id IN (",
    );
    for (index, thread_id) in thread_ids.iter().enumerate() {
        if index > 0 {
            metadata_statement.push(", ");
        }
        metadata_statement.push_bind(thread_id);
    }
    metadata_statement.push(")");
    let metadata_rows = metadata_statement.build().fetch_all(pool).await?;
    let mut current_heads = HashMap::with_capacity(metadata_rows.len());
    for row in metadata_rows {
        let thread_id: i64 = row.try_get("thread_id")?;
        let payload_json: String = row.try_get("payload_json")?;
        let metadata: PullRequestMetadata = serde_json::from_str(&payload_json)?;
        current_heads.insert(thread_id, metadata.head.sha.as_str().to_owned());
    }

    let mut statement = QueryBuilder::<Sqlite>::new(
        "SELECT c.thread_id, c.family, c.state_json, c.source_clock_state, c.source_clock_us, h.head_sha AS snapshot_head_sha FROM family_coverage c LEFT JOIN thread_family_head_contexts h ON h.thread_id = c.thread_id AND h.family = c.family WHERE c.thread_id IN (",
    );
    for (index, thread_id) in thread_ids.iter().enumerate() {
        if index > 0 {
            statement.push(", ");
        }
        statement.push_bind(thread_id);
    }
    statement.push(")");
    let rows = statement.build().fetch_all(pool).await?;
    let mut coverage = HashMap::with_capacity(thread_ids.len());
    for row in rows {
        let thread_id: i64 = row.try_get("thread_id")?;
        let family: String = row.try_get("family")?;
        let state_json: String = row.try_get("state_json")?;
        let source_clock_state: String = row.try_get("source_clock_state")?;
        let source_clock_us: Option<i64> = row.try_get("source_clock_us")?;
        let snapshot_head_sha: Option<String> = row.try_get("snapshot_head_sha")?;
        let family = parse_evidence_family(&family)?;
        let state = serde_json::from_str(&state_json)?;
        coverage
            .entry(thread_id)
            .or_insert_with(HashMap::new)
            .insert(
                family,
                StoredCoverage {
                    state,
                    source_clock_state,
                    source_clock_us,
                    snapshot_head_sha,
                    current_head_sha: current_heads.get(&thread_id).cloned(),
                },
            );
    }
    Ok(coverage)
}

pub(crate) fn coverage_for_kind(
    discussion: &Discussion,
    stored: Option<&HashMap<EvidenceFamily, StoredCoverage>>,
) -> Vec<Coverage> {
    ALL_FAMILIES
        .into_iter()
        .filter(|family| {
            discussion.kind == ThreadKind::PullRequest || !is_pull_request_family(*family)
        })
        .map(|family| {
            let item = stored.and_then(|coverage| coverage.get(&family));
            let state = item
                .map(|item| item.state.clone())
                .unwrap_or(CoverageState::Missing);
            Coverage::new(family, state).with_stale(is_stale(discussion, family, item))
        })
        .collect()
}

fn is_stale(
    discussion: &Discussion,
    family: EvidenceFamily,
    stored: Option<&StoredCoverage>,
) -> bool {
    if !matches!(
        family,
        EvidenceFamily::Comments | EvidenceFamily::Reviews | EvidenceFamily::ReviewThreads
    ) {
        return false;
    }
    let Some(stored) = stored else {
        return false;
    };
    if matches!(stored.state, CoverageState::Missing) {
        return false;
    }

    let parent_clock_matches = stored.source_clock_state == "valid"
        && stored.source_clock_us == Some(discussion.updated_at.unix_microseconds());
    if !parent_clock_matches {
        return true;
    }
    match family {
        EvidenceFamily::Comments => match stored.state {
            CoverageState::Complete { item_count, .. } => {
                comment_count(discussion) != Some(item_count)
            }
            _ => false,
        },
        EvidenceFamily::Reviews | EvidenceFamily::ReviewThreads => {
            stored.snapshot_head_sha != stored.current_head_sha
        }
        _ => false,
    }
}

fn comment_count(discussion: &Discussion) -> Option<u64> {
    discussion
        .provider_data
        .get("comments")
        .and_then(serde_json::Value::as_u64)
}

fn build_thread_timeline(
    discussion: &Discussion,
    comments: &[StagedItem<Comment>],
    reviews: &[StagedItem<Review>],
    review_threads: &[StagedItem<ReviewThread>],
) -> Vec<ThreadTimelineEntry> {
    let mut entries = vec![ThreadTimelineEntry {
        occurred_at: Some(discussion.created_at),
        event: ThreadTimelineEvent::ThreadCreated {
            thread: discussion.id.clone(),
            title: discussion.title.clone(),
        },
    }];
    if let Some(closed_at) = discussion.closed_at {
        entries.push(ThreadTimelineEntry {
            occurred_at: Some(closed_at),
            event: ThreadTimelineEvent::ThreadClosed {
                thread: discussion.id.clone(),
            },
        });
    }
    entries.extend(comments.iter().map(|item| ThreadTimelineEntry {
        occurred_at: Some(item.payload.created_at),
        event: ThreadTimelineEvent::Comment {
            comment: item.payload.clone(),
        },
    }));
    entries.extend(reviews.iter().map(|item| ThreadTimelineEntry {
        occurred_at: item.payload.submitted_at,
        event: ThreadTimelineEvent::Review {
            review: item.payload.clone(),
        },
    }));
    for item in review_threads {
        let review_thread = &item.payload;
        let review_thread_id = review_thread.id.clone();
        entries.push(ThreadTimelineEntry {
            occurred_at: None,
            event: ThreadTimelineEvent::ReviewThread {
                id: review_thread_id.clone(),
                head_sha: review_thread.head_sha.clone(),
                is_resolved: review_thread.is_resolved,
                is_outdated: review_thread.is_outdated,
                path: review_thread.path.clone(),
            },
        });
        entries.extend(
            review_thread
                .comments
                .iter()
                .map(|comment| ThreadTimelineEntry {
                    occurred_at: Some(comment.created_at),
                    event: ThreadTimelineEvent::ReviewThreadComment {
                        review_thread_id: review_thread_id.clone(),
                        head_sha: review_thread.head_sha.clone(),
                        is_resolved: review_thread.is_resolved,
                        is_outdated: review_thread.is_outdated,
                        path: review_thread.path.clone(),
                        comment: comment.clone(),
                    },
                }),
        );
    }
    entries.sort_by(|left, right| {
        compare_timeline_time(left.occurred_at, right.occurred_at)
            .then_with(|| timeline_event_key(&left.event).cmp(&timeline_event_key(&right.event)))
    });
    entries
}

fn compare_timeline_time(
    left: Option<UtcTimestamp>,
    right: Option<UtcTimestamp>,
) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn timeline_event_key(event: &ThreadTimelineEvent) -> (u8, String) {
    match event {
        ThreadTimelineEvent::ThreadCreated { thread, .. }
        | ThreadTimelineEvent::ThreadClosed { thread } => {
            (0, thread.provider_id().as_str().to_owned())
        }
        ThreadTimelineEvent::Comment { comment } => {
            (1, comment.id.provider_id().as_str().to_owned())
        }
        ThreadTimelineEvent::Review { review } => (2, review.id.provider_id().as_str().to_owned()),
        ThreadTimelineEvent::ReviewThread { id, .. } => (3, id.provider_id().as_str().to_owned()),
        ThreadTimelineEvent::ReviewThreadComment {
            review_thread_id,
            comment,
            ..
        } => (
            4,
            format!(
                "{}:{}",
                review_thread_id.provider_id().as_str(),
                comment.id.provider_id().as_str()
            ),
        ),
    }
}

async fn load_family_members<T>(
    pool: &sqlx::SqlitePool,
    thread_id: i64,
    family: &str,
) -> Result<Vec<StagedItem<T>>, StoreError>
where
    T: DeserializeOwned,
{
    let rows = sqlx::query(
        "SELECT provider_id, payload_json FROM thread_family_membership WHERE thread_id = ? AND family = ? ORDER BY provider_id",
    )
    .bind(thread_id)
    .bind(family)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            let id: String = row.try_get("provider_id")?;
            let payload_json: String = row.try_get("payload_json")?;
            let id = forgesync_core::identity::ProviderId::new(id)
                .map_err(|_| StoreError::InvalidStoredProviderId)?;
            Ok(StagedItem {
                id,
                payload: serde_json::from_str(&payload_json)?,
            })
        })
        .collect()
}

fn is_pull_request_family(family: EvidenceFamily) -> bool {
    matches!(
        family,
        EvidenceFamily::PullRequestMetadata
            | EvidenceFamily::Reviews
            | EvidenceFamily::ReviewThreads
    )
}

fn evidence_family_name(family: EvidenceFamily) -> &'static str {
    match family {
        EvidenceFamily::Threads => "threads",
        EvidenceFamily::Comments => "comments",
        EvidenceFamily::PullRequestMetadata => "pull_request_metadata",
        EvidenceFamily::Reviews => "reviews",
        EvidenceFamily::ReviewThreads => "review_threads",
    }
}

fn parse_evidence_family(value: &str) -> Result<EvidenceFamily, StoreError> {
    match value {
        "threads" => Ok(EvidenceFamily::Threads),
        "comments" => Ok(EvidenceFamily::Comments),
        "pull_request_metadata" => Ok(EvidenceFamily::PullRequestMetadata),
        "reviews" => Ok(EvidenceFamily::Reviews),
        "review_threads" => Ok(EvidenceFamily::ReviewThreads),
        _ => Err(StoreError::InvalidStoredCoverage),
    }
}

fn is_fts_syntax_error(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|database| database.message().to_ascii_lowercase().contains("fts5"))
}
