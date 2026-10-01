//! Human wording for timeline events, after the detail view's timestamp prefix.

use forgesync_core::content::{Comment, Review};
use forgesync_core::identity::ThreadId;
use forgesync_store::reads::ThreadTimelineEvent;

use crate::reports::threads::review_state_name;

/// Formats one event; the caller supplies the timestamp and section layout.
pub fn timeline_summary(event: &ThreadTimelineEvent) -> String {
    match event {
        ThreadTimelineEvent::ThreadCreated { thread, title } => opened_summary(thread, title),
        ThreadTimelineEvent::ThreadClosed { thread } => closed_summary(thread),
        ThreadTimelineEvent::Comment { comment } => comment_summary(comment),
        ThreadTimelineEvent::Review { review } => review_summary(review),
        ThreadTimelineEvent::ReviewThread {
            path,
            is_resolved,
            is_outdated,
            ..
        } => {
            let context = ReviewThreadContext {
                path: path.as_deref(),
                is_resolved: *is_resolved,
                is_outdated: *is_outdated,
            };
            context.summary()
        }
        ThreadTimelineEvent::ReviewThreadComment {
            path,
            is_resolved,
            is_outdated,
            comment,
            ..
        } => {
            let context = ReviewThreadContext {
                path: path.as_deref(),
                is_resolved: *is_resolved,
                is_outdated: *is_outdated,
            };
            context.comment_summary(comment)
        }
    }
}

/// Labels creation with the provider-qualified discussion and its title.
fn opened_summary(thread: &ThreadId, title: &str) -> String {
    format!(
        "{}#{} opened: {title}",
        thread.repository().provider_id(),
        thread.number().get()
    )
}

fn closed_summary(thread: &ThreadId) -> String {
    format!(
        "{}#{} closed",
        thread.repository().provider_id(),
        thread.number().get()
    )
}

/// Labels an unavailable author explicitly and keeps the source body.
fn comment_summary(comment: &Comment) -> String {
    let author = comment.author.as_deref().unwrap_or("unknown author");
    format!("comment by {author}: {}", comment.body)
}

/// Describes the reviewer, review state, and optional body.
fn review_summary(review: &Review) -> String {
    let reviewer = review
        .reviewer
        .as_ref()
        .and_then(|reviewer| reviewer.login.as_deref())
        .unwrap_or("unknown reviewer");
    let state = review_state_name(&review.state);
    let body = review
        .body
        .as_deref()
        .map_or(String::new(), |body| format!(" — {body}"));
    format!(
        "review {} by {reviewer}: {state}{body}",
        review.id.provider_id()
    )
}

/// Source review-thread facts shared by thread-state and comment projections.
struct ReviewThreadContext<'a> {
    path: Option<&'a str>,
    is_resolved: bool,
    is_outdated: bool,
}

impl ReviewThreadContext<'_> {
    fn summary(&self) -> String {
        format!(
            "review thread {}: {}{}",
            self.path(),
            self.resolution(),
            self.outdated_suffix()
        )
    }

    /// Attaches the thread context to one review comment.
    fn comment_summary(&self, comment: &Comment) -> String {
        let author = comment.author.as_deref().unwrap_or("unknown author");
        format!(
            "review comment on {} ({}{}), by {author}: {}",
            self.path(),
            self.resolution(),
            self.outdated_suffix(),
            comment.body
        )
    }

    /// Labels absent code context explicitly.
    fn path(&self) -> &str {
        self.path.unwrap_or("unknown path")
    }

    fn resolution(&self) -> &'static str {
        if self.is_resolved {
            "resolved"
        } else {
            "unresolved"
        }
    }

    fn outdated_suffix(&self) -> &'static str {
        if self.is_outdated { ", outdated" } else { "" }
    }
}

#[cfg(test)]
mod tests {
    use crate::reports::timeline::ReviewThreadContext;

    #[test]
    fn resolved_outdated_context_retains_both_source_facts() {
        let context = ReviewThreadContext {
            path: Some("src/lib.rs"),
            is_resolved: true,
            is_outdated: true,
        };
        assert_eq!(
            context.summary(),
            "review thread src/lib.rs: resolved, outdated"
        );
    }

    #[test]
    fn missing_path_is_explicit_without_inventing_outdated_state() {
        let context = ReviewThreadContext {
            path: None,
            is_resolved: false,
            is_outdated: false,
        };
        assert_eq!(context.summary(), "review thread unknown path: unresolved");
    }
}
