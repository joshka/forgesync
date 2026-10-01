//! One line per current-evidence timeline event, with explicit fallbacks for missing data.

use forgesync_core::content::{Comment, Review};
use forgesync_store::reads::ThreadTimelineEvent;
use ratatui::text::Line;

/// Formats one current-evidence event.
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

fn created_line(title: &str) -> Line<'static> {
    Line::from(format!("Created: {title}"))
}

fn closed_line() -> Line<'static> {
    Line::from("Discussion closed")
}

fn comment_line(comment: &Comment) -> Line<'static> {
    let author = comment.author.as_deref().unwrap_or("unknown author");
    Line::from(format!("Comment · {author}: {}", comment.body))
}

fn review_line(review: &Review) -> Line<'static> {
    let reviewer = review
        .reviewer
        .as_ref()
        .and_then(|reviewer| reviewer.login.as_deref())
        .unwrap_or("unknown reviewer");
    let body = review.body.as_deref().unwrap_or("(no review comment)");
    Line::from(format!("Review · {reviewer} · {body}"))
}

fn review_comment_line(comment: &Comment, path: Option<&str>) -> Line<'static> {
    let path = path.unwrap_or("unknown path");
    let author = comment.author.as_deref().unwrap_or("unknown author");
    Line::from(format!(
        "Review comment · {path} · {author}: {}",
        comment.body
    ))
}

struct ReviewThreadView<'a> {
    path: Option<&'a str>,
    is_resolved: bool,
    is_outdated: bool,
}

impl ReviewThreadView<'_> {
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
