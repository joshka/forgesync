//! Render recipe-defined retrieval text.
//!
//! Changing assembly changes content hashes and later embeddings, so ordering and whitespace are
//! observable contracts. Supplied detail relationships are trusted rather than revalidated.

use forgesync_core::content::{Comment, Review, ReviewState, ReviewThread, ThreadKind};
use forgesync_core::coverage::{CoverageState, EvidenceFamily};
use forgesync_core::document::{Document, DocumentRecipe};
use forgesync_store::observations::StagedItem;
use forgesync_store::reads::ThreadDetail;
use serde_json::Value;

/// Renders the selected recipe from the supplied local detail without I/O.
///
/// Preserves readable source text after boundary trimming and deterministic child ordering.
/// Enriched child sections require explicit complete, nonstale coverage; missing or stale families
/// are omitted rather than inferred from loaded rows. The supplied detail is trusted as a
/// projection, and construction does not recheck archive currentness or provider parentage.
///
/// Produces recipe identity, content hash, normalized deduplication text, and the discussion update
/// clock through [`Document::new`]. It performs no embedding or persistence.
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
        if has_current_complete_evidence(detail, EvidenceFamily::Comments) {
            append_comments(&mut sections, &detail.comments);
        }
        if discussion.kind == ThreadKind::PullRequest {
            if has_current_complete_evidence(detail, EvidenceFamily::Reviews) {
                append_reviews(&mut sections, &detail.reviews);
            }
            if has_current_complete_evidence(detail, EvidenceFamily::ReviewThreads) {
                append_review_threads(&mut sections, &detail.review_threads);
            }
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

/// Includes child evidence in a document only when the archive says its collection is complete
/// and still describes the current parent discussion.
fn has_current_complete_evidence(detail: &ThreadDetail, family: EvidenceFamily) -> bool {
    detail
        .summary
        .coverage
        .iter()
        .find(|coverage| coverage.family() == family)
        .is_some_and(|coverage| {
            !coverage.is_stale() && matches!(coverage.state(), CoverageState::Complete { .. })
        })
}

/// Adds current discussion comments to the selected document recipe.
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

/// Adds ordered non-bot review state and text to readable document sections.
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

/// Adds current review threads and their nested comments to document text.
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

/// Collects the visible comments for one normalized review thread.
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

/// Keeps author attribution with nonempty text in a document section.
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

/// Omits empty text so recipe output stays stable across missing fields.
fn push_nonempty(sections: &mut Vec<String>, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        sections.push(value.to_owned());
    }
}

/// Recognizes automated authors for the selected document recipe.
fn is_bot(user: &Option<&Value>) -> bool {
    user.and_then(|user| user.get("type"))
        .and_then(Value::as_str)
        .is_some_and(|kind| kind.eq_ignore_ascii_case("bot"))
}

/// Produces lowercase whitespace-normalized deduplication text without altering readable sections.
fn normalize_for_deduplication(text: &str) -> String {
    text.replace('\0', " ")
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Converts a normalized review state to its document label.
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
