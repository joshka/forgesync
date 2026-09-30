//! # Present current discussion evidence as terminal lines
//!
//! `timeline_line` selects wording for the store's typed current-evidence events. Detail section
//! assembly and scroll bounds remain in `view::detail`; this module explains one event at a time.
//! It preserves the supplied body, author, path, and reviewer data and performs no archive read.
//!
//! Comment and review helpers name missing-author and missing-body fallbacks. `ReviewThreadView`
//! keeps path, resolution, and outdated source facts together for review-thread wording. Those
//! boolean fields describe provider facts, rather than switches that choose unrelated behavior.
//! Review-comment wording uses only its path and comment, matching the existing display contract.
//!
//! These lines describe current acquired evidence, not source revision history. Event ordering
//! comes from the store projection and is preserved by the caller's traversal.

use forgesync_core::content::{Comment, Review};
use forgesync_store::reads::ThreadTimelineEvent;
use ratatui::text::Line;

/// Selects one event projection, leaving wording and fallback policy with named local owners.
pub fn timeline_line(event: &ThreadTimelineEvent) -> Line<'static> {
    match event {
        ThreadTimelineEvent::ThreadCreated { title, .. } => created_line(title),
        ThreadTimelineEvent::ThreadClosed { .. } => closed_line(),
        ThreadTimelineEvent::Comment { comment } => comment_line(comment),
        ThreadTimelineEvent::Review { review } => review_line(review),
        ThreadTimelineEvent::ReviewThread {
            is_resolved,
            is_outdated,
            path,
            ..
        } => {
            let view = ReviewThreadView {
                path: path.as_deref(),
                is_resolved: *is_resolved,
                is_outdated: *is_outdated,
            };
            view.line()
        }
        ThreadTimelineEvent::ReviewThreadComment { comment, path, .. } => {
            review_comment_line(comment, path.as_deref())
        }
    }
}

/// Shows the observed title associated with discussion creation.
fn created_line(title: &str) -> Line<'static> {
    Line::from(format!("Created: {title}"))
}

/// Marks closure independently of comment or review evidence.
fn closed_line() -> Line<'static> {
    Line::from("Discussion closed")
}

/// Preserves comment body text and marks an absent author explicitly.
fn comment_line(comment: &Comment) -> Line<'static> {
    let author = comment.author.as_deref().unwrap_or("unknown author");
    Line::from(format!("Comment · {author}: {}", comment.body))
}

/// Shows reviewer identity and body, retaining distinct missing-reviewer and missing-body labels.
fn review_line(review: &Review) -> Line<'static> {
    let reviewer = review
        .reviewer
        .as_ref()
        .and_then(|reviewer| reviewer.login.as_deref())
        .unwrap_or("unknown reviewer");
    let body = review.body.as_deref().unwrap_or("(no review comment)");
    Line::from(format!("Review · {reviewer} · {body}"))
}

/// Associates a review comment with its source path without inferring thread resolution.
fn review_comment_line(comment: &Comment, path: Option<&str>) -> Line<'static> {
    let path = path.unwrap_or("unknown path");
    let author = comment.author.as_deref().unwrap_or("unknown author");
    Line::from(format!(
        "Review comment · {path} · {author}: {}",
        comment.body
    ))
}

/// Source facts retained together while presenting one review thread.
struct ReviewThreadView<'a> {
    /// Source file path; absent paths receive an explicit display fallback.
    path: Option<&'a str>,
    /// Provider-reported resolution, independent of whether the source location is outdated.
    is_resolved: bool,
    /// Provider-reported outdated location, independent of local acquisition completeness.
    is_outdated: bool,
}

impl ReviewThreadView<'_> {
    /// Keeps resolution, location freshness, and path as three distinct display facts.
    fn line(&self) -> Line<'static> {
        let resolution = if self.is_resolved {
            "resolved"
        } else {
            "unresolved"
        };
        let freshness = if self.is_outdated {
            "outdated"
        } else {
            "current"
        };
        let path = self.path.unwrap_or("unknown path");
        Line::from(format!(
            "Review thread · {resolution} · {freshness} · {path}"
        ))
    }
}

#[cfg(test)]
mod tests {
    //! Independent source facts retain their labels and explicit missing-path fallback.

    use ratatui::text::Line;

    use crate::view::timeline::ReviewThreadView;

    #[test]
    fn resolved_outdated_thread_keeps_source_path() {
        let view = ReviewThreadView {
            path: Some("src/lib.rs"),
            is_resolved: true,
            is_outdated: true,
        };
        assert_eq!(
            view.line(),
            Line::from("Review thread · resolved · outdated · src/lib.rs")
        );
    }

    #[test]
    fn unresolved_current_thread_marks_missing_path() {
        let view = ReviewThreadView {
            path: None,
            is_resolved: false,
            is_outdated: false,
        };
        assert_eq!(
            view.line(),
            Line::from("Review thread · unresolved · current · unknown path")
        );
    }
}
