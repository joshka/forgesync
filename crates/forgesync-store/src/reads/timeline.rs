//! # Project selected evidence into a stable discussion timeline
//!
//! `thread_timeline` combines parent creation/closure, comments, reviews, and review-thread
//! evidence already selected by the archive. It does not infer collection completeness or acquire
//! missing resources; discussion coverage is reported separately by the detail projection.
//!
//! Entry constructors keep each event's timestamp and payload together. Review-thread state has
//! no occurrence timestamp, while its comments retain their source creation times. Parent state
//! and review-thread context are copied into the corresponding events for presentation.
//!
//! Ordering uses known source times first, then stable event-kind/provider identity keys. Unknown
//! timestamps follow dated events. Equal keys retain insertion order, preserving the established
//! creation/closure behavior when a provider supplies equal timestamps.

use forgesync_core::content::{Comment, Discussion, Review, ReviewThread};
use forgesync_core::timestamp::UtcTimestamp;

use crate::observations::StagedItem;
use crate::reads::{ThreadTimelineEntry, ThreadTimelineEvent};

/// Combines selected family evidence and applies source-time and stable-identity ordering.
pub fn thread_timeline(
    discussion: &Discussion,
    comments: &[StagedItem<Comment>],
    reviews: &[StagedItem<Review>],
    review_threads: &[StagedItem<ReviewThread>],
) -> Vec<ThreadTimelineEntry> {
    let mut entries = vec![ThreadTimelineEntry::created(discussion)];
    if let Some(closed_at) = discussion.closed_at {
        entries.push(ThreadTimelineEntry::closed(discussion, closed_at));
    }
    entries.extend(
        comments
            .iter()
            .map(|item| ThreadTimelineEntry::comment(&item.payload)),
    );
    entries.extend(
        reviews
            .iter()
            .map(|item| ThreadTimelineEntry::review(&item.payload)),
    );
    for item in review_threads {
        entries.push(ThreadTimelineEntry::review_thread(&item.payload));
        entries.extend(
            item.payload
                .comments
                .iter()
                .map(|comment| ThreadTimelineEntry::review_thread_comment(&item.payload, comment)),
        );
    }
    entries.sort_by(entry_order);
    entries
}

impl ThreadTimelineEntry {
    /// Projects the parent creation event using its original source time.
    fn created(discussion: &Discussion) -> Self {
        Self {
            occurred_at: Some(discussion.created_at),
            event: ThreadTimelineEvent::ThreadCreated {
                thread: discussion.id.clone(),
                title: discussion.title.clone(),
            },
        }
    }

    /// Projects a recorded parent closure without inventing a time for an open discussion.
    fn closed(discussion: &Discussion, closed_at: UtcTimestamp) -> Self {
        Self {
            occurred_at: Some(closed_at),
            event: ThreadTimelineEvent::ThreadClosed {
                thread: discussion.id.clone(),
            },
        }
    }

    /// Keeps a selected comment and its source creation time in one event.
    fn comment(comment: &Comment) -> Self {
        Self {
            occurred_at: Some(comment.created_at),
            event: ThreadTimelineEvent::Comment {
                comment: comment.clone(),
            },
        }
    }

    /// Preserves an absent provider submission time rather than substituting acquisition time.
    fn review(review: &Review) -> Self {
        Self {
            occurred_at: review.submitted_at,
            event: ThreadTimelineEvent::Review {
                review: review.clone(),
            },
        }
    }

    /// Projects review-thread state without assigning an unsupported occurrence time.
    fn review_thread(thread: &ReviewThread) -> Self {
        Self {
            occurred_at: None,
            event: ThreadTimelineEvent::ReviewThread {
                id: thread.id.clone(),
                head_sha: thread.head_sha.clone(),
                is_resolved: thread.is_resolved,
                is_outdated: thread.is_outdated,
                path: thread.path.clone(),
            },
        }
    }

    /// Attaches selected review-thread context to a comment at its source creation time.
    fn review_thread_comment(thread: &ReviewThread, comment: &Comment) -> Self {
        Self {
            occurred_at: Some(comment.created_at),
            event: ThreadTimelineEvent::ReviewThreadComment {
                review_thread_id: thread.id.clone(),
                head_sha: thread.head_sha.clone(),
                is_resolved: thread.is_resolved,
                is_outdated: thread.is_outdated,
                path: thread.path.clone(),
                comment: comment.clone(),
            },
        }
    }
}

/// Orders source times before applying the established stable event identity tie-breaker.
fn entry_order(left: &ThreadTimelineEntry, right: &ThreadTimelineEntry) -> std::cmp::Ordering {
    compare_timeline_time(left.occurred_at, right.occurred_at)
        .then_with(|| timeline_event_key(&left.event).cmp(&timeline_event_key(&right.event)))
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

#[cfg(test)]
mod tests {
    //! # Optional source-time ordering
    //!
    //! Dated events precede undated events in both comparison directions.
    //! Two absent times compare equal, leaving the surrounding comparator to resolve identity ties.
    //! A fixed timestamp provides one present value; no process clock or archive setup is involved.
    //! The inverse comparisons describe one ordering contract and remain together.
    //!
    //! These cases exercise only the optional-time helper, not the full timeline projection.
    //! Integration tests own event hydration, family coverage, filtering, and pagination.
    //! Event identities and source payloads are intentionally absent from this local arithmetic
    //! test. Changes to tie-breaking policy belong beside the comparator that actually owns
    //! those facts.

    use std::cmp::Ordering;

    use forgesync_core::timestamp::UtcTimestamp;

    use crate::reads::timeline::compare_timeline_time;

    #[test]
    fn dated_events_precede_undated_events() {
        let time = UtcTimestamp::parse("2026-09-20T12:00:00Z").expect("timestamp");
        assert_eq!(compare_timeline_time(Some(time), None), Ordering::Less);
        assert_eq!(compare_timeline_time(None, Some(time)), Ordering::Greater);
    }

    #[test]
    fn absent_times_leave_ordering_to_the_event_identity() {
        assert_eq!(compare_timeline_time(None, None), Ordering::Equal);
    }
}
