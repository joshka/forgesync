//! # Reciprocal-rank fusion with retained source evidence
//!
//! `fuse_hybrid` combines keyword and semantic candidates by durable discussion identity. Each
//! source contributes its reciprocal rank, rather than mixing a text score with cosine similarity.
//! `HybridRanking` owns the combined identity map; `FusionEntry` owns one discussion's evidence.
//!
//! Keyword summaries are inserted first and retained when the semantic source finds the same
//! discussion. Semantic rank and cosine evidence travel together as `SemanticEvidence`. Results
//! expose keyword provenance before semantic provenance, keeping their separate explanations.
//!
//! Ranking ties use stable thread identity, so hash-map iteration cannot change output order.
//! Explicit created/updated sorting still retains fused scores and provenance. Truncation follows
//! ordering; final pagination remains in `ranking`. This module performs no archive or provider
//! I/O.

use std::cmp::Ordering;
use std::collections::HashMap;

use forgesync_core::identity::ThreadId;
use forgesync_store::reads::ThreadSummary;

use crate::exact_search::{ScoredThread, stable_thread_id_cmp};
use crate::inspect::ThreadSort;
use crate::search::{SearchHit, SearchProvenance};

/// Rank smoothing constant shared by both sources; a first-place hit contributes `1 / 61`.
const RRF_CONSTANT: f64 = 60.0;

/// Combines source ranks, orders ties deterministically, and bounds the retained candidate window.
pub fn fuse_hybrid(
    keyword: Vec<SearchHit>,
    semantic: Vec<ScoredThread>,
    sort: ThreadSort,
    limit: usize,
) -> Vec<SearchHit> {
    let mut ranking = HybridRanking {
        entries: HashMap::with_capacity(keyword.len() + semantic.len()),
    };
    ranking.add_keyword(keyword);
    ranking.add_semantic(semantic);
    let mut hits = ranking.into_hits();
    hits.sort_by(|left, right| hit_order(left, right, sort));
    hits.truncate(limit);
    hits
}

/// Identity-based union of two independently ranked candidate sources.
struct HybridRanking {
    /// First summary and latest source ranks for each identity, before final deterministic
    /// sorting.
    entries: HashMap<ThreadId, FusionEntry>,
}

impl HybridRanking {
    /// Retains keyword summaries and records one-based source ranks in candidate order.
    fn add_keyword(&mut self, hits: Vec<SearchHit>) {
        for (index, hit) in hits.into_iter().enumerate() {
            let rank = source_rank(index);
            self.entries
                .entry(hit.summary.discussion.id.clone())
                .and_modify(|entry| entry.keyword_rank = Some(rank))
                .or_insert(FusionEntry {
                    summary: hit.summary,
                    keyword_rank: Some(rank),
                    semantic: None,
                });
        }
    }

    /// Attaches semantic evidence without replacing a summary already supplied by either source.
    fn add_semantic(&mut self, candidates: Vec<ScoredThread>) {
        for (index, candidate) in candidates.into_iter().enumerate() {
            let evidence = SemanticEvidence {
                rank: source_rank(index),
                cosine_score: candidate.score,
            };
            self.entries
                .entry(candidate.summary.discussion.id.clone())
                .and_modify(|entry| entry.semantic = Some(evidence))
                .or_insert(FusionEntry {
                    summary: candidate.summary,
                    keyword_rank: None,
                    semantic: Some(evidence),
                });
        }
    }

    /// Projects entries into scored public hits; the caller sorts before exposing map iteration.
    fn into_hits(self) -> Vec<SearchHit> {
        self.entries
            .into_values()
            .map(FusionEntry::into_hit)
            .collect()
    }
}

/// One retained summary and the independent rank evidence explaining its fused score.
struct FusionEntry {
    /// First source summary; keyword candidates take precedence because they are inserted first.
    summary: ThreadSummary,
    /// Optional one-based keyword position; absent for a semantic-only discussion.
    keyword_rank: Option<u32>,
    /// Coupled semantic position and cosine explanation; absent for a keyword-only discussion.
    semantic: Option<SemanticEvidence>,
}

impl FusionEntry {
    /// Adds rank contributions and emits source explanations in keyword-then-semantic order.
    fn into_hit(self) -> SearchHit {
        let keyword_score = self.keyword_rank.map(reciprocal_rank_score).unwrap_or(0.0);
        let semantic_score = self
            .semantic
            .map(|evidence| reciprocal_rank_score(evidence.rank))
            .unwrap_or(0.0);
        let mut provenance = Vec::with_capacity(2);
        if let Some(rank) = self.keyword_rank {
            provenance.push(SearchProvenance::Keyword { rank });
        }
        if let Some(evidence) = self.semantic {
            provenance.push(SearchProvenance::Semantic {
                rank: evidence.rank,
                cosine_score: evidence.cosine_score,
            });
        }
        SearchHit {
            summary: self.summary,
            score: Some(keyword_score + semantic_score),
            provenance,
        }
    }
}

/// Semantic rank and cosine evidence belonging to the same source candidate.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SemanticEvidence {
    /// One-based semantic candidate position used for the fused contribution.
    rank: u32,
    /// Original cosine score preserved for explanation, not added to the rank score.
    cosine_score: f64,
}

/// Orders by the requested primary field and then stable discussion identity.
fn hit_order(left: &SearchHit, right: &SearchHit, sort: ThreadSort) -> Ordering {
    let primary = match sort {
        ThreadSort::Relevance => right
            .score
            .unwrap_or_default()
            .total_cmp(&left.score.unwrap_or_default()),
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

/// Converts a zero-based candidate position into the bounded one-based rank representation.
fn source_rank(index: usize) -> u32 {
    u32::try_from(index + 1).unwrap_or(u32::MAX)
}

/// Converts one source rank to its reciprocal-rank fusion contribution.
pub fn reciprocal_rank_score(rank: u32) -> f64 {
    1.0 / (RRF_CONSTANT + f64::from(rank))
}
