//! Best-chunk relevance, stable identity ties, positive-score filtering, and cancellation.

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::coverage::Coverage;
use forgesync_core::embedding::EmbeddingVector;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::embeddings::{EmbeddingSearchDocument, StoredEmbeddingChunk};
use forgesync_store::reads::{ThreadSort, ThreadSummary};
use tokio_util::sync::CancellationToken;

use crate::scoring::{ScoredThread, TopScored};

#[test]
fn relevance_uses_the_best_chunk_in_each_document() {
    let query = vector(&[1.0, 0.0]);
    let results = rank(
        query,
        vec![
            candidate(3, &[&[0.2, 0.8], &[1.0, 0.0]]),
            candidate(1, &[&[0.8, 0.6]]),
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
        [3, 1]
    );
    assert_eq!(results[0].score, 1.0);
    assert!((results[1].score - 0.8).abs() < 1e-6);
}

#[test]
fn equal_scores_are_ordered_by_stable_identity() {
    let query = vector(&[1.0, 0.0]);
    let results = rank(
        query,
        vec![candidate(2, &[&[0.8, 0.6]]), candidate(1, &[&[0.8, 0.6]])],
        ThreadSort::Relevance,
        10,
        &CancellationToken::new(),
    )
    .expect("rank tied vectors");

    assert_eq!(results[0].summary.discussion.id.number().get(), 1);
    assert_eq!(results[1].summary.discussion.id.number().get(), 2);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].score, results[1].score);
}

#[test]
fn zero_similarity_candidates_are_excluded() {
    let query = vector(&[1.0, 0.0]);
    let results = rank(
        query,
        vec![candidate(1, &[&[0.0, 1.0]])],
        ThreadSort::Relevance,
        10,
        &CancellationToken::new(),
    )
    .expect("score orthogonal vector");

    assert!(results.is_empty());
}

#[test]
fn already_cancelled_scoring_returns_cancellation() {
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let error = rank(
        vector(&[1.0, 0.0]),
        vec![candidate(1, &[&[1.0, 0.0]])],
        ThreadSort::Relevance,
        10,
        &cancellation,
    )
    .expect_err("cancelled ranking");
    assert_eq!(error.code(), "operation_cancelled");
}

/// Ranks one batch of documents through the bounded scorer.
fn rank(
    query: EmbeddingVector,
    documents: Vec<EmbeddingSearchDocument>,
    sort: ThreadSort,
    limit: usize,
    cancellation: &CancellationToken,
) -> Result<Vec<ScoredThread>, crate::error::EngineError> {
    let mut ranking = TopScored::new(query, sort, limit);
    ranking.add(documents, cancellation)?;
    Ok(ranking.finish().0)
}

/// Validates supplied components without normalizing or calculating expected similarity.
fn vector(values: &[f32]) -> EmbeddingVector {
    EmbeddingVector::new(values.to_vec(), None).expect("valid vector")
}

/// Constructs one issue with fixed timestamps and caller-supplied chunks in input order.
///
/// Chunk mapping supplies consecutive coordinates and a common count; it performs no ranking,
/// archive writes, or model requests. The number determines both display and provider identity.
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
