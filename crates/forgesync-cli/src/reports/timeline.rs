//! # Human wording for selected timeline evidence
//!
//! `timeline_summary` dispatches each stored event to its named text projection. Discussion detail
//! owns section headings and timestamps; this module formats only the evidence after that prefix.
//! It does not load source content or define JSON fields.
//!
//! `ReviewThreadContext` keeps path, resolution, and outdated state together for both thread-state
//! and thread-comment wording. These are source facts, not behavioral switches. Missing authors,
//! reviewers, or paths receive the established human labels; absent review bodies add no suffix.
//!
//! Event payloads remain typed store projections. Formatting does not establish completeness or
//! revision history. Source body text retains its existing spelling and line breaks.

use forgesync_core::content::{Comment, Review};
use forgesync_core::identity::ThreadId;
use forgesync_store::reads::ThreadTimelineEvent;

use crate::reports::threads::review_state_name;

/// Formats one selected event without controlling timestamps or surrounding section layout.
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

/// Shows the stable provider repository identity and current discussion title at creation.
fn opened_summary(thread: &ThreadId, title: &str) -> String {
    format!(
        "{}#{} opened: {title}",
        thread.repository().provider_id(),
        thread.number().get()
    )
}

/// Shows the same provider-qualified discussion label when closure evidence exists.
fn closed_summary(thread: &ThreadId) -> String {
    format!(
        "{}#{} closed",
        thread.repository().provider_id(),
        thread.number().get()
    )
}

/// Retains source comment body and explicitly labels an unavailable author.
fn comment_summary(comment: &Comment) -> String {
    let author = comment.author.as_deref().unwrap_or("unknown author");
    format!("comment by {author}: {}", comment.body)
}

/// Describes reviewer identity, review state, and an optional source body.
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
    /// Provider path, absent when source context did not include one.
    path: Option<&'a str>,
    /// Whether the selected review-thread snapshot was resolved.
    is_resolved: bool,
    /// Whether the selected snapshot described an outdated code context.
    is_outdated: bool,
}

impl ReviewThreadContext<'_> {
    /// Describes current thread state without inventing an occurrence time.
    fn summary(&self) -> String {
        format!(
            "review thread {}: {}{}",
            self.path(),
            self.resolution(),
            self.outdated_suffix()
        )
    }

    /// Attaches the same context to the selected review comment and source author/body.
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

    /// Labels absent code context explicitly instead of omitting its position in the wording.
    fn path(&self) -> &str {
        self.path.unwrap_or("unknown path")
    }

    /// Maps the recorded resolution fact to the established human label.
    fn resolution(&self) -> &'static str {
        if self.is_resolved {
            "resolved"
        } else {
            "unresolved"
        }
    }

    /// Adds outdated context only when the selected source snapshot records it.
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
