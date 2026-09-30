//! # Exact scoring scenarios
//!
//! These tests exercise vector arithmetic, best-chunk relevance ordering, stable identity ties,
//! positive-score filtering, and cancellation before candidate scoring. Candidate construction
//! supplies fixed valid domain records; it does not compute expected ranking or run provider I/O.
//! All candidate timestamps are equal, so these scenarios establish relevance ordering rather
//! than the separate created/updated ordering policies.

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::coverage::Coverage;
use forgesync_core::embedding::EmbeddingVector;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::embeddings::{EmbeddingSearchDocument, StoredEmbeddingChunk};
use forgesync_store::reads::{ThreadSort, ThreadSummary};
use tokio_util::sync::CancellationToken;

use crate::exact_search::cosine_similarity;
use crate::scoring::score_embedding_page;

#[rstest::rstest]
#[case::parallel(&[1.0, 0.0], 1.0)]
#[case::orthogonal(&[0.0, 1.0], 0.0)]
#[case::opposite(&[-1.0, 0.0], -1.0)]
fn cosine_similarity_matches_axis_directions(#[case] values: &[f32], #[case] expected: f64) {
    let left = EmbeddingVector::new(vec![1.0, 0.0], None).expect("valid left vector");
    let right = EmbeddingVector::new(values.to_vec(), None).expect("valid right vector");

    assert_eq!(cosine_similarity(&left, &right), Some(expected));
}

#[test]
fn cosine_similarity_matches_diagonal_direction() {
    let left = EmbeddingVector::new(vec![1.0, 0.0], None).expect("valid left vector");
    let right = EmbeddingVector::new(vec![0.5, 0.5], None).expect("valid right vector");

    let score = cosine_similarity(&left, &right).expect("comparable vectors");
    let difference = (score - std::f64::consts::FRAC_1_SQRT_2).abs();
    assert!(difference < 1e-7, "diagonal score difference: {difference}");
}

#[test]
fn cosine_similarity_rejects_dimension_mismatch() {
    let left = EmbeddingVector::new(vec![1.0, 0.0], None).expect("valid left vector");
    let right = EmbeddingVector::new(vec![1.0], None).expect("valid right vector");

    assert_eq!(cosine_similarity(&left, &right), None);
}

#[test]
fn cosine_similarity_handles_large_finite_components() {
    let left = EmbeddingVector::new(vec![f32::MAX, f32::MAX], None).expect("finite vector");
    let right = EmbeddingVector::new(vec![f32::MAX, f32::MAX], None).expect("finite vector");

    let score = cosine_similarity(&left, &right).expect("comparable vectors");
    assert!((score - 1.0).abs() < 1e-12, "parallel score: {score}");
}

#[test]
fn exact_search_ranks_chunk_maxima_and_breaks_score_ties_by_stable_identity() {
    let query = vector(&[1.0, 0.0]);
    let results = score_embedding_page(
        &query,
        vec![
            candidate(3, &[&[0.2, 0.8], &[1.0, 0.0]]),
            candidate(2, &[&[0.8, 0.6]]),
            candidate(1, &[&[0.8, 0.6]]),
            candidate(4, &[&[0.0, 1.0]]),
        ],
        ThreadSort::Relevance,
        10,
        &CancellationToken::new(),
    )
    .expect("rank vectors");

    assert_eq!(
        results
            .iter()
            .map(|result| result.summary.discussion.id.number().get())
            .collect::<Vec<_>>(),
        [3, 1, 2]
    );
    assert_eq!(results[0].score, 1.0);
    assert!((results[1].score - 0.8).abs() < 1e-6);
}

#[test]
fn exact_search_observes_cancellation_between_candidates() {
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let error = score_embedding_page(
        &vector(&[1.0, 0.0]),
        vec![candidate(1, &[&[1.0, 0.0]])],
        ThreadSort::Relevance,
        10,
        &cancellation,
    )
    .expect_err("cancelled ranking");
    assert_eq!(error.code(), "operation_cancelled");
}

fn vector(values: &[f32]) -> EmbeddingVector {
    EmbeddingVector::new(values.to_vec(), None).expect("valid vector")
}

fn candidate(number: u64, vectors: &[&[f32]]) -> EmbeddingSearchDocument {
    let host = GitHubHost::parse("github.com").expect("host");
    let repository_id = RepositoryId::new(
        host,
        ProviderId::new("repo-1").expect("repository provider ID"),
    );
    let identity = ThreadId::new(
        repository_id.clone(),
        ProviderId::new(format!("thread-{number}")).expect("thread provider ID"),
        ThreadNumber::new(number).expect("thread number"),
    );
    let timestamp = UtcTimestamp::parse("2026-09-28T00:00:00Z").expect("timestamp");
    let summary = ThreadSummary {
        repository: Repository {
            id: repository_id,
            owner: "owner".to_owned(),
            name: "repo".to_owned(),
            full_name: "owner/repo".to_owned(),
            default_branch: None,
            updated_at: Some(timestamp),
            provider_data: ProviderData::default(),
        },
        discussion: Discussion {
            id: identity,
            kind: ThreadKind::Issue,
            state: SourceState::Open,
            title: format!("thread {number}"),
            body: None,
            html_url: None,
            created_at: timestamp,
            updated_at: timestamp,
            closed_at: None,
            labels: Vec::new(),
            assignees: Vec::new(),
            provider_data: ProviderData::default(),
        },
        coverage: Vec::<Coverage>::new(),
    };
    let count = u32::try_from(vectors.len()).expect("chunk count");
    let chunks = vectors
        .iter()
        .enumerate()
        .map(|(index, values)| StoredEmbeddingChunk {
            index: u32::try_from(index).expect("chunk index"),
            count,
            chunk_hash: format!("{:064x}", index + 1),
            vector: vector(values),
        })
        .collect();
    EmbeddingSearchDocument { summary, chunks }
}
