//! Coverage archive reads.

use super::*;

impl Archive {
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
