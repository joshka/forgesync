use std::cmp::Ordering;

use forgesync_core::embedding::EmbeddingVector;
use forgesync_store::embeddings::EmbeddingSearchDocument;
use forgesync_store::reads::{ThreadSort, ThreadSummary};
use tokio_util::sync::CancellationToken;

use crate::EngineError;

#[derive(Clone, Debug)]
pub(crate) struct ScoredThread {
    pub summary: ThreadSummary,
    pub score: f64,
}

/// Computes cosine similarity using scaled f64 accumulation to avoid f32 overflow.
pub fn cosine_similarity(left: &EmbeddingVector, right: &EmbeddingVector) -> Option<f64> {
    if left.dimensions() != right.dimensions() {
        return None;
    }
    let left_values = left.values();
    let right_values = right.values();
    let mut left_scale = 0.0_f64;
    let mut right_scale = 0.0_f64;
    for (&left_value, &right_value) in left_values.iter().zip(right_values) {
        left_scale = left_scale.max(f64::from(left_value).abs());
        right_scale = right_scale.max(f64::from(right_value).abs());
    }
    if left_scale == 0.0 || right_scale == 0.0 {
        return None;
    }

    let mut dot = 0.0_f64;
    let mut left_norm = 0.0_f64;
    let mut right_norm = 0.0_f64;
    for (&left_value, &right_value) in left_values.iter().zip(right_values) {
        let left_value = f64::from(left_value) / left_scale;
        let right_value = f64::from(right_value) / right_scale;
        dot += left_value * right_value;
        left_norm += left_value * left_value;
        right_norm += right_value * right_value;
    }
    let score = dot / (left_norm.sqrt() * right_norm.sqrt());
    Some(score.clamp(-1.0, 1.0))
}

pub(crate) fn score_embedding_page(
    query: &EmbeddingVector,
    documents: Vec<EmbeddingSearchDocument>,
    sort: ThreadSort,
    limit: usize,
    cancellation: &CancellationToken,
) -> Result<Vec<ScoredThread>, EngineError> {
    let mut scored = Vec::with_capacity(documents.len().min(limit));
    for document in documents {
        if cancellation.is_cancelled() {
            return Err(EngineError::SearchCancelled);
        }
        if document
            .chunks
            .iter()
            .any(|chunk| chunk.vector.dimensions() != query.dimensions())
        {
            continue;
        }
        let score = document
            .chunks
            .iter()
            .filter_map(|chunk| cosine_similarity(query, &chunk.vector))
            .fold(f64::NEG_INFINITY, f64::max);
        if !score.is_finite() || score <= 0.0 {
            continue;
        }
        scored.push(ScoredThread {
            summary: document.summary,
            score,
        });
    }
    sort_scored(&mut scored, sort);
    scored.truncate(limit);
    Ok(scored)
}

pub(crate) fn merge_scored_pages(
    current: &mut Vec<ScoredThread>,
    page: Vec<ScoredThread>,
    sort: ThreadSort,
    limit: usize,
) {
    current.extend(page);
    sort_scored(current, sort);
    current.truncate(limit);
}

pub(crate) fn sort_scored(scored: &mut [ScoredThread], sort: ThreadSort) {
    scored.sort_by(|left, right| compare_scored(left, right, sort));
}

fn compare_scored(left: &ScoredThread, right: &ScoredThread, sort: ThreadSort) -> Ordering {
    let primary = match sort {
        ThreadSort::Relevance => right.score.total_cmp(&left.score),
        ThreadSort::Updated => right
            .summary
            .discussion
            .updated_at
            .cmp(&left.summary.discussion.updated_at),
        ThreadSort::Created => right
            .summary
            .discussion
            .created_at
            .cmp(&left.summary.discussion.created_at),
    };
    primary.then_with(|| stable_thread_id_cmp(&left.summary, &right.summary))
}

pub(crate) fn stable_thread_id_cmp(left: &ThreadSummary, right: &ThreadSummary) -> Ordering {
    let left_id = &left.discussion.id;
    let right_id = &right.discussion.id;
    left_id
        .repository()
        .host()
        .cmp(right_id.repository().host())
        .then_with(|| {
            left_id
                .repository()
                .provider_id()
                .cmp(right_id.repository().provider_id())
        })
        .then_with(|| left_id.provider_id().cmp(right_id.provider_id()))
        .then_with(|| left_id.number().cmp(&right_id.number()))
}

#[cfg(test)]
mod tests {
    use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
    use forgesync_core::coverage::Coverage;
    use forgesync_core::embedding::EmbeddingVector;
    use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
    use forgesync_core::provider_data::ProviderData;
    use forgesync_core::timestamp::UtcTimestamp;
    use forgesync_store::embeddings::{EmbeddingSearchDocument, StoredEmbeddingChunk};
    use forgesync_store::reads::{ThreadSort, ThreadSummary};
    use tokio_util::sync::CancellationToken;

    use super::{cosine_similarity, score_embedding_page};

    #[test]
    fn cosine_similarity_handles_known_directions_and_dimension_mismatch() {
        let same = vector(&[1.0, 0.0]);
        let diagonal = vector(&[0.5, 0.5]);
        let orthogonal = vector(&[0.0, 1.0]);
        let opposite = vector(&[-1.0, 0.0]);

        assert_eq!(cosine_similarity(&same, &same), Some(1.0));
        assert_eq!(cosine_similarity(&same, &orthogonal), Some(0.0));
        assert_eq!(cosine_similarity(&same, &opposite), Some(-1.0));
        assert!(
            (cosine_similarity(&same, &diagonal).expect("score") - std::f64::consts::FRAC_1_SQRT_2)
                .abs()
                < 1e-7
        );
        assert_eq!(cosine_similarity(&same, &vector(&[1.0])), None);
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
}
