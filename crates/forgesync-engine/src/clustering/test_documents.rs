//! # Static archived-document fixtures for clustering policy tests
//!
//! `document` creates one open discussion with a single checked embedding chunk. Its explicit
//! inputs expose the fields that vary between graph scenarios: identity number, kind, title, body,
//! and vector. Repository identity, acquisition time, coverage, and chunk metadata are fixed.
//!
//! Both graph-selection and proposal-projection tests use this fixture. It performs no provider or
//! archive I/O, selects no behavior based on a test name, and supplies no assertions. The scenario
//! remains visible at the caller; the fixture only avoids repeating normalized domain construction.
//! Tests pass valid vector values because they exercise clustering policy rather than validation.

use forgesync_core::content::{Discussion, Repository, SourceState, ThreadKind};
use forgesync_core::coverage::Coverage;
use forgesync_core::embedding::EmbeddingVector;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId, ThreadId, ThreadNumber};
use forgesync_core::provider_data::ProviderData;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::embeddings::{EmbeddingSearchDocument, StoredEmbeddingChunk};
use forgesync_store::reads::ThreadSummary;

/// Builds static normalized display content and one compatible chunk from explicit scenario facts.
pub fn document(
    number: u64,
    kind: ThreadKind,
    title: &str,
    body: Option<&str>,
    vector_values: &[f32],
) -> EmbeddingSearchDocument {
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
            owner: "example".to_owned(),
            name: "repo".to_owned(),
            full_name: "example/repo".to_owned(),
            default_branch: None,
            updated_at: Some(timestamp),
            provider_data: ProviderData::default(),
        },
        discussion: Discussion {
            id: identity,
            kind,
            state: SourceState::Open,
            title: title.to_owned(),
            body: body.map(str::to_owned),
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
    let vector = EmbeddingVector::new(vector_values.to_vec(), None).expect("vector");
    EmbeddingSearchDocument {
        summary,
        chunks: vec![StoredEmbeddingChunk {
            index: 0,
            count: 1,
            chunk_hash: format!("{:064x}", number),
            vector,
        }],
    }
}
