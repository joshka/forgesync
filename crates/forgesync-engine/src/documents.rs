use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forgesync_core::{
    Comment, Document, DocumentRecipe, Review, ReviewState, ReviewThread, ThreadKind, UtcTimestamp,
};
use forgesync_store::{Archive, DocumentWrite, StagedItem, ThreadDetail};
use serde::Serialize;
use serde_json::Value;

use crate::{EngineError, ThreadSelector, show_thread};

/// Result of building and saving a current thread document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DocumentBuildReport {
    /// Generated deterministic input.
    pub document: Document,
    /// Persistence result and embedding invalidation signal.
    pub write: DocumentWrite,
}

/// Builds one deterministic retrieval document from current normalized thread evidence.
pub fn build_document(detail: &ThreadDetail, recipe: DocumentRecipe) -> Document {
    let discussion = &detail.summary.discussion;
    let title = discussion.title.trim().to_owned();
    let mut sections = Vec::new();
    if !title.is_empty() {
        sections.push(format!("# {title}"));
    }
    push_nonempty(&mut sections, discussion.body.as_deref());

    let labels = discussion
        .labels
        .iter()
        .map(|label| label.trim())
        .filter(|label| !label.is_empty())
        .collect::<Vec<_>>();
    if !labels.is_empty() {
        sections.push(format!("Labels: {}", labels.join(", ")));
    }

    if recipe == DocumentRecipe::DiscussionEnriched {
        append_comments(&mut sections, &detail.comments);
        if discussion.kind == ThreadKind::PullRequest {
            append_reviews(&mut sections, &detail.reviews);
            append_review_threads(&mut sections, &detail.review_threads);
        }
    }

    let text = sections.join("\n\n");
    let dedupe_text = normalize_for_deduplication(&text);
    Document::new(
        discussion.id.clone(),
        recipe,
        title,
        text,
        dedupe_text,
        discussion.updated_at,
    )
}

/// Builds the selected recipe from one locally archived discussion.
pub async fn build_thread_document(
    archive: &Archive,
    reference: &ThreadSelector,
    recipe: DocumentRecipe,
) -> Result<Document, EngineError> {
    let detail = show_thread(archive, reference).await?;
    Ok(build_document(&detail, recipe))
}

/// Builds and stores one document under the archive writer lease.
pub async fn materialize_thread_document(
    archive: &Archive,
    reference: &ThreadSelector,
    recipe: DocumentRecipe,
) -> Result<DocumentBuildReport, EngineError> {
    let document = build_thread_document(archive, reference, recipe).await?;
    let built_at = now_utc()?;
    let lease = archive
        .acquire_archive_lease(built_at, Duration::from_secs(60))
        .await?;
    let write = archive
        .upsert_document_fenced(&lease, &document, built_at)
        .await;
    let release = archive.release_archive_lease(&lease, built_at).await;
    let write = match write {
        Ok(write) => write,
        Err(error) => return Err(error.into()),
    };
    release?;
    Ok(DocumentBuildReport { document, write })
}

fn now_utc() -> Result<UtcTimestamp, EngineError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| forgesync_store::StoreError::ClockOutOfRange)?;
    let micros = i64::try_from(elapsed.as_micros())
        .map_err(|_| forgesync_store::StoreError::ClockOutOfRange)?;
    UtcTimestamp::from_unix_microseconds(micros)
        .map_err(forgesync_store::StoreError::InvalidCreatedAt)
        .map_err(Into::into)
}

fn append_comments(sections: &mut Vec<String>, comments: &[StagedItem<Comment>]) {
    let mut ordered = comments.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.payload
            .created_at
            .cmp(&right.payload.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    for comment in ordered {
        if is_bot(&comment.payload.provider_data.get("user")) {
            continue;
        }
        push_attributed_body(
            sections,
            "Comment",
            comment.payload.author.as_deref(),
            &comment.payload.body,
        );
    }
}

fn append_reviews(sections: &mut Vec<String>, reviews: &[StagedItem<Review>]) {
    let mut ordered = reviews.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.payload
            .submitted_at
            .is_none()
            .cmp(&right.payload.submitted_at.is_none())
            .then_with(|| left.payload.submitted_at.cmp(&right.payload.submitted_at))
            .then_with(|| left.id.cmp(&right.id))
    });
    for review in ordered {
        if is_bot(&review.payload.provider_data.get("user")) {
            continue;
        }
        let reviewer = review
            .payload
            .reviewer
            .as_ref()
            .and_then(|reviewer| reviewer.login.as_deref())
            .unwrap_or("unknown reviewer");
        let state = review_state_label(&review.payload.state);
        let body = review.payload.body.as_deref().unwrap_or_default().trim();
        let section = if body.is_empty() {
            format!("Review by {reviewer}: {state}")
        } else {
            format!("Review by {reviewer}: {state}\n\n{}", body)
        };
        sections.push(section);
    }
}

fn append_review_threads(sections: &mut Vec<String>, review_threads: &[StagedItem<ReviewThread>]) {
    let mut ordered = review_threads.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.payload
            .path
            .cmp(&right.payload.path)
            .then_with(|| left.payload.line.cmp(&right.payload.line))
            .then_with(|| left.id.cmp(&right.id))
    });
    for review_thread in ordered {
        let path = review_thread
            .payload
            .path
            .as_deref()
            .unwrap_or("unknown path");
        let resolution = if review_thread.payload.is_resolved {
            "resolved"
        } else {
            "unresolved"
        };
        let age = if review_thread.payload.is_outdated {
            "outdated"
        } else {
            "current"
        };
        sections.push(format!("Review thread on {path} ({resolution}, {age})"));
        append_comments(sections, &review_thread_comments(review_thread));
    }
}

fn review_thread_comments(thread: &StagedItem<ReviewThread>) -> Vec<StagedItem<Comment>> {
    thread
        .payload
        .comments
        .iter()
        .map(|comment| StagedItem {
            id: comment.id.provider_id().clone(),
            payload: comment.clone(),
        })
        .collect()
}

fn push_attributed_body(sections: &mut Vec<String>, kind: &str, author: Option<&str>, body: &str) {
    let body = body.trim();
    if body.is_empty() {
        return;
    }
    let author = author.filter(|author| !author.trim().is_empty());
    let attribution =
        author.map_or_else(|| kind.to_owned(), |author| format!("{kind} by {author}"));
    sections.push(format!("{attribution}:\n\n{body}"));
}

fn push_nonempty(sections: &mut Vec<String>, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        sections.push(value.to_owned());
    }
}

fn is_bot(user: &Option<&Value>) -> bool {
    user.and_then(|user| user.get("type"))
        .and_then(Value::as_str)
        .is_some_and(|kind| kind.eq_ignore_ascii_case("bot"))
}

fn normalize_for_deduplication(text: &str) -> String {
    text.replace('\0', " ")
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn review_state_label(state: &ReviewState) -> &str {
    match state {
        ReviewState::Approved => "approved",
        ReviewState::ChangesRequested => "requested changes",
        ReviewState::Commented => "commented",
        ReviewState::Dismissed => "dismissed",
        ReviewState::Pending => "pending",
        ReviewState::Other(state) => state,
    }
}

#[cfg(test)]
mod tests {
    use forgesync_core::{
        BranchRef, Comment, CommentId, CommitSha, DocumentRecipe, GitHubHost, ProviderData,
        ProviderId, PullRequestMetadata, Repository, RepositoryId, Review, ReviewId, ReviewState,
        ReviewThread, ReviewThreadId, ReviewerIdentity, SourceState, ThreadId, ThreadKind,
        ThreadNumber, UtcTimestamp,
    };
    use forgesync_store::{StagedItem, ThreadDetail, ThreadSummary};
    use serde_json::json;

    use super::build_document;

    #[test]
    fn original_body_recipe_excludes_discussion_and_review_evidence() {
        let detail = sample_detail();
        let document = build_document(&detail, DocumentRecipe::OriginalBody);

        assert_eq!(document.title, "A selected change");
        assert!(document.text.contains("Body text."));
        assert!(document.text.contains("Labels: bug"));
        assert!(!document.text.contains("Discussion reply"));
        assert!(!document.text.contains("Review requested changes"));
        assert!(!document.text.contains("Review thread on src/lib.rs"));
    }

    #[test]
    fn discussion_recipe_orders_selected_evidence_and_skips_bot_comments() {
        let detail = sample_detail();
        let document = build_document(&detail, DocumentRecipe::DiscussionEnriched);
        let first = document.text.find("Earlier reply").expect("earlier reply");
        let second = document.text.find("Discussion reply").expect("later reply");

        assert!(first < second);
        assert!(
            document
                .text
                .contains("Review by reviewer: requested changes")
        );
        assert!(
            document
                .text
                .contains("Review thread on src/lib.rs (unresolved, current)")
        );
        assert!(document.text.contains("Inline suggestion"));
        assert!(!document.text.contains("Automated notice"));
        assert_eq!(document.dedupe_text, document.dedupe_text.to_lowercase());
    }

    fn sample_detail() -> ThreadDetail {
        let repository_id = RepositoryId::new(
            GitHubHost::parse("github.com").expect("host"),
            ProviderId::new("41").expect("repository ID"),
        );
        let thread_id = ThreadId::new(
            repository_id.clone(),
            ProviderId::new("9001").expect("thread ID"),
            ThreadNumber::new(12).expect("thread number"),
        );
        let repository = Repository {
            id: repository_id,
            owner: "owner".to_owned(),
            name: "repo".to_owned(),
            full_name: "owner/repo".to_owned(),
            default_branch: Some("main".to_owned()),
            updated_at: None,
            provider_data: ProviderData::new(),
        };
        let discussion = forgesync_core::Discussion {
            id: thread_id.clone(),
            kind: ThreadKind::PullRequest,
            state: SourceState::Open,
            title: "A selected change".to_owned(),
            body: Some("Body text.".to_owned()),
            html_url: None,
            created_at: timestamp("2026-09-19T08:00:00Z"),
            updated_at: timestamp("2026-09-20T08:00:00Z"),
            closed_at: None,
            labels: vec!["bug".to_owned()],
            assignees: Vec::new(),
            provider_data: ProviderData::new(),
        };
        let original_comment = Comment {
            id: CommentId::new(thread_id.clone(), ProviderId::new("1").expect("comment ID")),
            review_id: None,
            author: Some("reviewer".to_owned()),
            body: "Discussion reply".to_owned(),
            created_at: timestamp("2026-09-20T10:00:00Z"),
            updated_at: None,
            provider_data: ProviderData::new(),
        };
        let earlier_comment = Comment {
            id: CommentId::new(thread_id.clone(), ProviderId::new("2").expect("comment ID")),
            review_id: None,
            author: Some("reviewer".to_owned()),
            body: "Earlier reply".to_owned(),
            created_at: timestamp("2026-09-20T09:00:00Z"),
            updated_at: None,
            provider_data: ProviderData::new(),
        };
        let mut bot_data = ProviderData::new();
        bot_data.insert("user", json!({"type": "Bot"}));
        let bot_comment = Comment {
            id: CommentId::new(thread_id.clone(), ProviderId::new("3").expect("comment ID")),
            review_id: None,
            author: Some("robot".to_owned()),
            body: "Automated notice".to_owned(),
            created_at: timestamp("2026-09-20T11:00:00Z"),
            updated_at: None,
            provider_data: bot_data,
        };
        let review = Review {
            id: ReviewId::new(thread_id.clone(), ProviderId::new("4").expect("review ID")),
            state: ReviewState::ChangesRequested,
            reviewer: Some(ReviewerIdentity {
                provider_id: None,
                login: Some("reviewer".to_owned()),
                provider_data: ProviderData::new(),
            }),
            body: Some("Please make this change.".to_owned()),
            submitted_at: Some(timestamp("2026-09-20T12:00:00Z")),
            commit_sha: None,
            provider_data: ProviderData::new(),
        };
        let inline_comment = Comment {
            id: CommentId::new(thread_id.clone(), ProviderId::new("5").expect("comment ID")),
            review_id: None,
            author: Some("reviewer".to_owned()),
            body: "Inline suggestion".to_owned(),
            created_at: timestamp("2026-09-20T13:00:00Z"),
            updated_at: None,
            provider_data: ProviderData::new(),
        };
        let review_thread = ReviewThread {
            id: ReviewThreadId::new(
                thread_id.clone(),
                ProviderId::new("6").expect("review thread ID"),
            ),
            head_sha: CommitSha::new("a".repeat(40)).expect("head SHA"),
            is_resolved: false,
            is_outdated: false,
            path: Some("src/lib.rs".to_owned()),
            line: Some(32),
            comments: vec![inline_comment],
            provider_data: ProviderData::new(),
        };
        let metadata = PullRequestMetadata {
            base: BranchRef {
                name: "main".to_owned(),
                sha: CommitSha::new("b".repeat(40)).expect("base SHA"),
                repository: None,
            },
            head: BranchRef {
                name: "topic".to_owned(),
                sha: CommitSha::new("a".repeat(40)).expect("head SHA"),
                repository: None,
            },
            draft: false,
            merged: false,
            provider_data: ProviderData::new(),
        };

        ThreadDetail {
            summary: ThreadSummary {
                repository,
                discussion,
                coverage: Vec::new(),
            },
            comments: vec![
                staged("1", original_comment),
                staged("3", bot_comment),
                staged("2", earlier_comment),
            ],
            pull_request_metadata: vec![staged("7", metadata)],
            reviews: vec![staged("4", review)],
            review_threads: vec![staged("6", review_thread)],
            timeline: Vec::new(),
        }
    }

    fn staged<T>(id: &str, payload: T) -> StagedItem<T> {
        StagedItem {
            id: ProviderId::new(id).expect("staged item ID"),
            payload,
        }
    }

    fn timestamp(value: &str) -> UtcTimestamp {
        UtcTimestamp::parse(value).expect("timestamp")
    }
}
