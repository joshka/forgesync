//! Detail archive reads.

use sqlx::Row;

use super::{
    Archive, Comment, DeserializeOwned, Discussion, Review, ReviewThread, StagedItem, StoreError,
    ThreadDetail, ThreadReference, ThreadSummary, ThreadTimelineEntry, ThreadTimelineEvent,
    UtcTimestamp, coverage_for_kind, load_thread_coverage,
};

impl Archive {
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
}

/// Combines selected family evidence into one stable discussion timeline.
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

/// Orders timeline events by source time when available.
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

/// Breaks equal timeline timestamps with a stable event identity.
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

/// Loads current complete family membership for a discussion.
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
