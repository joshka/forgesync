use forgesync_core::content::{
    BranchRef, Comment, PullRequestMetadata, Repository, Review, ReviewState, ReviewThread,
    ReviewerIdentity, SourceState, ThreadKind,
};
use forgesync_core::coverage::{Coverage, CoverageState, EvidenceFamily};
use forgesync_core::document::DocumentRecipe;
use forgesync_core::identity::{
    CommentId, CommitSha, GitHubHost, ObservationSequence, ProviderId, RepositoryId, ReviewId,
    ReviewThreadId, ThreadId, ThreadNumber,
};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::observations::StagedItem;
use forgesync_store::reads::{ThreadDetail, ThreadSummary};
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

#[test]
fn discussion_recipe_excludes_stale_family_evidence() {
    let mut detail = sample_detail();
    detail.summary.coverage = vec![
        complete_coverage(EvidenceFamily::Comments, 3).with_stale(true),
        complete_coverage(EvidenceFamily::Reviews, 1).with_stale(true),
        complete_coverage(EvidenceFamily::ReviewThreads, 1),
    ];

    let document = build_document(&detail, DocumentRecipe::DiscussionEnriched);

    assert!(!document.text.contains("Discussion reply"));
    assert!(!document.text.contains("Review by reviewer"));
    assert!(document.text.contains("Review thread on src/lib.rs"));
    assert!(document.text.contains("Inline suggestion"));
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
    let discussion = forgesync_core::content::Discussion {
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
            coverage: vec![
                complete_coverage(EvidenceFamily::Comments, 3),
                complete_coverage(EvidenceFamily::Reviews, 1),
                complete_coverage(EvidenceFamily::ReviewThreads, 1),
            ],
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

fn complete_coverage(family: EvidenceFamily, item_count: u64) -> Coverage {
    Coverage::new(
        family,
        CoverageState::Complete {
            observed_at: timestamp("2026-09-20T14:00:00Z"),
            sequence: ObservationSequence::new(1).expect("sequence"),
            item_count,
        },
    )
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
