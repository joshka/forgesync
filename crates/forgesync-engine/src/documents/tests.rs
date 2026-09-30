//! # Document recipe evidence
//!
//! These tests show which pieces of a thread enter each search-document recipe. Original-body and
//! discussion recipes have different evidence boundaries, ordering, and bot-comment behavior.
//! Stale child-family evidence is excluded because a newly observed parent does not make every
//! child collection current. Read these examples before changing document text: a small ordering
//! change affects later embeddings and search results.

use forgesync_core::coverage::EvidenceFamily;
use forgesync_core::document::DocumentRecipe;

use crate::documents::build_document;
use crate::documents::test_detail::{complete_coverage, sample_detail};

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
fn discussion_recipe_orders_comments_by_source_time() {
    let detail = sample_detail();
    let document = build_document(&detail, DocumentRecipe::DiscussionEnriched);
    let first = document.text.find("Earlier reply").expect("earlier reply");
    let second = document.text.find("Discussion reply").expect("later reply");

    assert!(first < second);
}

#[test]
fn discussion_recipe_includes_review_and_inline_thread_evidence() {
    let detail = sample_detail();
    let document = build_document(&detail, DocumentRecipe::DiscussionEnriched);

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
}

#[test]
fn discussion_recipe_excludes_bot_comments() {
    let detail = sample_detail();
    let document = build_document(&detail, DocumentRecipe::DiscussionEnriched);

    assert!(!document.text.contains("Automated notice"));
    assert!(document.text.contains("Discussion reply"));
}

#[test]
fn discussion_recipe_normalizes_deduplication_case() {
    let detail = sample_detail();
    let document = build_document(&detail, DocumentRecipe::DiscussionEnriched);

    assert!(document.text.contains("A selected change"));
    assert_eq!(document.dedupe_text, document.dedupe_text.to_lowercase());
}

#[test]
fn discussion_recipe_excludes_stale_family_evidence() {
    let mut detail = sample_detail();
    detail.summary.coverage = vec![
        complete_coverage(EvidenceFamily::Comments, 3).mark_stale(),
        complete_coverage(EvidenceFamily::Reviews, 1).mark_stale(),
        complete_coverage(EvidenceFamily::ReviewThreads, 1),
    ];

    let document = build_document(&detail, DocumentRecipe::DiscussionEnriched);

    assert!(!document.text.contains("Discussion reply"));
    assert!(!document.text.contains("Review by reviewer"));
    assert!(document.text.contains("Review thread on src/lib.rs"));
    assert!(document.text.contains("Inline suggestion"));
}
