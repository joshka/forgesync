use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::{Arc, LazyLock, OnceLock};
use std::time::Duration;

use forgesync_core::document::DocumentRecipe;
use forgesync_core::identity::{RepositoryId, ThreadId};
use forgesync_store::archive::Archive;
use forgesync_store::clusters::{
    ClusterDetail, ClusterGenerationInput, ClusterGenerationResult, ClusterInput,
    ClusterListQuery as StoreClusterListQuery, ClusterMemberInput, ClusterPage,
};
use forgesync_store::embeddings::{EmbeddingDocumentQuery, EmbeddingSearchDocument};
use forgesync_store::leases::ArchiveLeaseToken;
use forgesync_store::reads::{ThreadQuery, ThreadSort, ThreadStateFilter, ThreadSummary};
use regex::Regex;
use serde::Serialize;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{Instant, interval_at};
use tokio_util::sync::CancellationToken;

use crate::EngineError;
use crate::documents::now_utc;
use crate::exact_search::{cosine_similarity, stable_thread_id_cmp};
use crate::inspect::{checked_page, resolve_repositories};
use crate::reference::{RepositorySelector, ThreadSelector};

const CLUSTER_PAGE_SIZE: u32 = 500;
const CLUSTER_LEASE_DURATION: Duration = Duration::from_secs(180);
const CLUSTER_WORKER_LIMIT: usize = 1;

static CLUSTER_WORKER_SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();

const DEFAULT_CLUSTER_THRESHOLD: f64 = 0.80;
const DEFAULT_CROSS_KIND_THRESHOLD: f64 = 0.93;
const HIGH_CONFIDENCE_SCORE: f64 = 0.90;
const MIN_TITLE_OVERLAP: f64 = 0.18;
const REFERENCE_SCORE: f64 = 0.94;
const EARLY_BODY_REFERENCE_BYTES: usize = 240;

static TITLE_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z0-9]{4,}").expect("valid title token pattern"));
static THREAD_REFERENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:\b([\w.-]+/[\w.-]+)#(\d+)|(?:\b([\w.-]+/[\w.-]+)/)?(?:issues|pull)/(\d+)|#(\d{2,}))")
        .expect("valid issue reference pattern")
});

/// Tuning options for deterministic related-discussion clustering.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClusterOptions {
    /// Minimum cosine similarity for an embedding edge.
    pub threshold: f64,
    /// Minimum cosine similarity for an issue-to-pull-request edge.
    pub cross_kind_threshold: f64,
    /// Maximum selected neighbors per discussion.
    pub fanout: usize,
    /// Maximum members in one generated component.
    pub max_cluster_size: usize,
    /// Minimum members emitted as a generated cluster.
    pub min_cluster_size: usize,
}

impl Default for ClusterOptions {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_CLUSTER_THRESHOLD,
            cross_kind_threshold: DEFAULT_CROSS_KIND_THRESHOLD,
            fanout: 16,
            max_cluster_size: 40,
            min_cluster_size: 1,
        }
    }
}

impl ClusterOptions {
    pub(crate) fn validate(self) -> Result<Self, EngineError> {
        if !self.threshold.is_finite()
            || !(0.0..=1.0).contains(&self.threshold)
            || !self.cross_kind_threshold.is_finite()
            || !(0.0..=1.0).contains(&self.cross_kind_threshold)
            || self.fanout == 0
            || self.fanout > 256
            || self.max_cluster_size == 0
            || self.max_cluster_size > 10_000
            || self.min_cluster_size == 0
            || self.min_cluster_size > self.max_cluster_size
        {
            return Err(EngineError::InvalidClusterOptions);
        }
        Ok(self)
    }
}

/// Explicit vector identity and graph policy for a local cluster generation.
#[derive(Clone, Debug)]
pub struct ClusterBuildRequest {
    /// Repository to cluster from current open discussions.
    pub repository: RepositorySelector,
    /// Canonical endpoint identity used when vectors were persisted, without credentials.
    pub endpoint: String,
    /// Model identity used when vectors were persisted.
    pub model: String,
    /// Versioned document recipe used when vectors were persisted.
    pub recipe: DocumentRecipe,
    /// Deterministic similarity and component limits.
    pub options: ClusterOptions,
}

/// Coverage and persistence result for one generated cluster run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ClusterBuildReport {
    /// Archive-local run and generated cluster counts.
    pub generation: ClusterGenerationResult,
    /// Number of current open discussions selected for this repository.
    pub eligible_threads: u64,
    /// Number of current open discussions with compatible stored vectors.
    pub vector_threads: u64,
    /// Candidate similarity and reference edges retained after neighbor pruning.
    pub candidate_edges: u64,
}

/// Repository filters and pagination for read-only cluster listing.
#[derive(Clone, Debug)]
pub struct ClusterListRequest {
    /// Repositories to include; empty selects every registered repository.
    pub repositories: Vec<RepositorySelector>,
    /// Include groups retired by a complete generation.
    pub include_retired: bool,
    /// Maximum result count, from 1 through 1000.
    pub limit: u32,
    /// Number of matching rows to skip.
    pub offset: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct ClusterMemberCandidate {
    pub summary: ThreadSummary,
    pub score_to_representative: Option<f64>,
}

#[derive(Clone, Debug)]
pub(crate) struct ClusterCandidate {
    pub representative: ThreadId,
    pub title: String,
    pub members: Vec<ClusterMemberCandidate>,
}

#[derive(Clone, Copy, Debug)]
struct Neighbor {
    node_index: usize,
    score: f64,
}

impl PartialEq for Neighbor {
    fn eq(&self, other: &Self) -> bool {
        self.node_index == other.node_index && self.score.total_cmp(&other.score) == Ordering::Equal
    }
}

impl Eq for Neighbor {}

impl PartialOrd for Neighbor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Neighbor {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .score
            .total_cmp(&self.score)
            .then_with(|| self.node_index.cmp(&other.node_index))
    }
}

#[derive(Clone, Copy, Debug)]
struct CandidateEdge {
    left: usize,
    right: usize,
    score: f64,
}

/// Builds sparse candidate components from current discussion vectors and references.
pub(crate) fn build_cluster_candidates(
    mut documents: Vec<EmbeddingSearchDocument>,
    repository_full_name: &str,
    options: ClusterOptions,
    cancellation: &CancellationToken,
) -> Result<(Vec<ClusterCandidate>, usize), EngineError> {
    let options = options.validate()?;
    documents.sort_by(|left, right| stable_thread_id_cmp(&left.summary, &right.summary));
    if cancellation.is_cancelled() {
        return Err(EngineError::ClusteringCancelled);
    }

    let thread_index = documents
        .iter()
        .enumerate()
        .map(|(index, document)| (document.summary.discussion.id.clone(), index))
        .collect::<HashMap<_, _>>();
    if thread_index.len() != documents.len() {
        return Err(EngineError::InvalidClusterInput);
    }
    let reference_edges = deterministic_reference_edges(&documents, repository_full_name);
    let title_overlaps = documents
        .iter()
        .map(|document| title_tokens(&document.summary.discussion.title))
        .collect::<Vec<_>>();
    let mut neighbors = (0..documents.len())
        .map(|_| BinaryHeap::with_capacity(options.fanout + 1))
        .collect::<Vec<_>>();

    for left in 0..documents.len() {
        if cancellation.is_cancelled() {
            return Err(EngineError::ClusteringCancelled);
        }
        for right in left + 1..documents.len() {
            if right % 1024 == 0 && cancellation.is_cancelled() {
                return Err(EngineError::ClusteringCancelled);
            }
            let left_discussion = &documents[left].summary.discussion;
            let right_discussion = &documents[right].summary.discussion;
            let similarity = document_similarity(&documents[left], &documents[right]);
            let similarity_is_candidate = similarity.is_some_and(|score| {
                score >= options.threshold
                    && (score >= HIGH_CONFIDENCE_SCORE
                        || overlap_ratio(&title_overlaps[left], &title_overlaps[right])
                            >= MIN_TITLE_OVERLAP)
                    && (left_discussion.kind == right_discussion.kind
                        || score >= options.cross_kind_threshold)
            });
            let reference_score = reference_edges.get(&(left, right)).copied();
            let Some(score) = (if similarity_is_candidate {
                max_score(similarity, reference_score)
            } else {
                reference_score
            }) else {
                continue;
            };
            offer_neighbor(
                &mut neighbors[left],
                Neighbor {
                    node_index: right,
                    score,
                },
                options.fanout,
            );
            offer_neighbor(
                &mut neighbors[right],
                Neighbor {
                    node_index: left,
                    score,
                },
                options.fanout,
            );
        }
    }

    let mut selected = HashSet::with_capacity(documents.len().saturating_mul(options.fanout));
    for (left, list) in neighbors.iter().enumerate() {
        for neighbor in list {
            selected.insert((left.min(neighbor.node_index), left.max(neighbor.node_index)));
        }
    }
    let mut edges = selected
        .into_iter()
        .map(|(left, right)| {
            let similarity = document_similarity(&documents[left], &documents[right]);
            let reference = reference_edges.get(&(left, right)).copied();
            let score = max_score(
                similarity.filter(|score| {
                    *score >= options.threshold
                        && (*score >= HIGH_CONFIDENCE_SCORE
                            || overlap_ratio(&title_overlaps[left], &title_overlaps[right])
                                >= MIN_TITLE_OVERLAP)
                        && (documents[left].summary.discussion.kind
                            == documents[right].summary.discussion.kind
                            || *score >= options.cross_kind_threshold)
                }),
                reference,
            )
            .expect("selected edge has a candidate score");
            CandidateEdge { left, right, score }
        })
        .collect::<Vec<_>>();
    edges.sort_by(compare_edges);
    let edge_count = edges.len();
    let (clusters, kept_edges) = bounded_components(&documents, &edges, options);
    let candidates = format_clusters(&documents, &clusters, &kept_edges, options.min_cluster_size);
    Ok((candidates, edge_count))
}

/// Builds and persists deterministic clusters from current open discussions and stored vectors.
///
/// The archive lease fences the vector snapshot and generation write from concurrent archive
/// mutations. Incomplete vector coverage produces a partial run, which cannot retire unseen
/// clusters.
pub async fn build_clusters(
    archive: &Archive,
    request: &ClusterBuildRequest,
    cancellation: &CancellationToken,
) -> Result<ClusterBuildReport, EngineError> {
    request.options.validate()?;
    if request.endpoint.trim().is_empty()
        || request.model.trim().is_empty()
        || request.endpoint.len() > 2048
        || request.model.len() > 512
    {
        return Err(EngineError::InvalidClusterInput);
    }
    if cancellation.is_cancelled() {
        return Err(EngineError::ClusteringCancelled);
    }

    let started_at = now_utc()?;
    let lease = archive
        .acquire_archive_lease(started_at, CLUSTER_LEASE_DURATION)
        .await?;
    let operation_cancellation = cancellation.child_token();
    let mut operation = Box::pin(execute_cluster_build(
        archive,
        request,
        &lease,
        &operation_cancellation,
    ));
    let heartbeat_period = CLUSTER_LEASE_DURATION / 3;
    let mut heartbeat = interval_at(Instant::now() + heartbeat_period, heartbeat_period);
    let operation_result = loop {
        tokio::select! {
            result = &mut operation => break result,
            _ = cancellation.cancelled() => {
                operation_cancellation.cancel();
                let _ = operation.await;
                break Err(EngineError::ClusteringCancelled);
            }
            _ = heartbeat.tick() => {
                let now = match now_utc() {
                    Ok(now) => now,
                    Err(error) => {
                        operation_cancellation.cancel();
                        let _ = operation.await;
                        break Err(error);
                    }
                };
                if let Err(error) = archive
                    .heartbeat_archive_lease(&lease, now, CLUSTER_LEASE_DURATION)
                    .await
                {
                    operation_cancellation.cancel();
                    let _ = operation.await;
                    break Err(error.into());
                }
            }
        }
    };
    finish_cluster_lease_result(archive, &lease, operation_result).await
}

/// Reads one page of persisted clusters without contacting GitHub or mutating the archive.
pub async fn list_clusters(
    archive: &Archive,
    request: &ClusterListRequest,
) -> Result<ClusterPage, EngineError> {
    let (limit, offset) = checked_page(request.limit, request.offset)?;
    let repositories = resolve_repositories(archive, &request.repositories).await?;
    let query = StoreClusterListQuery {
        repositories: &repositories,
        include_retired: request.include_retired,
        limit,
        offset,
    };
    Ok(archive.list_clusters(&query).await?)
}

/// Reads one persisted cluster and its current or excluded members.
pub async fn show_cluster(archive: &Archive, id: u64) -> Result<ClusterDetail, EngineError> {
    Ok(archive.cluster_detail(id).await?)
}

/// Records a local dismissal decision for one generated cluster.
pub async fn dismiss_cluster(archive: &Archive, id: u64, reason: &str) -> Result<(), EngineError> {
    let at = now_utc()?;
    let lease = archive
        .acquire_archive_lease(at, CLUSTER_LEASE_DURATION)
        .await?;
    let result = archive
        .set_cluster_dismissed_fenced(&lease, id, true, reason, at)
        .await;
    finish_cluster_lease(archive, &lease, result).await
}

/// Clears a local dismissal decision for one generated cluster.
pub async fn restore_cluster(archive: &Archive, id: u64) -> Result<(), EngineError> {
    let at = now_utc()?;
    let lease = archive
        .acquire_archive_lease(at, CLUSTER_LEASE_DURATION)
        .await?;
    let result = archive
        .set_cluster_dismissed_fenced(&lease, id, false, "", at)
        .await;
    finish_cluster_lease(archive, &lease, result).await
}

/// Excludes one current cluster member as a local maintainer decision.
pub async fn exclude_cluster_member(
    archive: &Archive,
    id: u64,
    reference: &ThreadSelector,
    reason: &str,
) -> Result<(), EngineError> {
    set_cluster_member_excluded(archive, id, reference, true, reason).await
}

/// Includes one previously excluded current cluster member.
pub async fn include_cluster_member(
    archive: &Archive,
    id: u64,
    reference: &ThreadSelector,
) -> Result<(), EngineError> {
    set_cluster_member_excluded(archive, id, reference, false, "").await
}

/// Selects a current cluster member as the local canonical discussion.
pub async fn set_canonical_cluster_member(
    archive: &Archive,
    id: u64,
    reference: &ThreadSelector,
) -> Result<(), EngineError> {
    let thread = crate::inspect::show_thread(archive, reference)
        .await?
        .summary
        .discussion
        .id;
    let at = now_utc()?;
    let lease = archive
        .acquire_archive_lease(at, CLUSTER_LEASE_DURATION)
        .await?;
    let result = archive
        .set_cluster_canonical_fenced(&lease, id, &thread, at)
        .await;
    finish_cluster_decision_lease(archive, &lease, result).await
}

async fn set_cluster_member_excluded(
    archive: &Archive,
    id: u64,
    reference: &ThreadSelector,
    excluded: bool,
    reason: &str,
) -> Result<(), EngineError> {
    let thread = crate::inspect::show_thread(archive, reference)
        .await?
        .summary
        .discussion
        .id;
    let at = now_utc()?;
    let lease = archive
        .acquire_archive_lease(at, CLUSTER_LEASE_DURATION)
        .await?;
    let result = archive
        .set_cluster_member_excluded_fenced(&lease, id, &thread, excluded, reason, at)
        .await;
    finish_cluster_decision_lease(archive, &lease, result).await
}

async fn execute_cluster_build(
    archive: &Archive,
    request: &ClusterBuildRequest,
    lease: &ArchiveLeaseToken,
    cancellation: &CancellationToken,
) -> Result<ClusterBuildReport, EngineError> {
    let endpoint = request.endpoint.trim();
    let model = request.model.trim();
    let repositories =
        resolve_repositories(archive, std::slice::from_ref(&request.repository)).await?;
    let repository = repositories
        .first()
        .cloned()
        .ok_or(EngineError::InvalidClusterInput)?;
    let eligible_threads = count_open_threads(archive, &repositories, cancellation).await?;
    let documents = load_cluster_vectors(
        archive,
        &repositories,
        endpoint,
        model,
        request.recipe,
        cancellation,
    )
    .await?;
    let vector_threads = u64::try_from(documents.len())
        .map_err(|_| forgesync_store::error::StoreError::IntegerOutOfRange)?;
    if documents.len() > usize::try_from(eligible_threads).unwrap_or(usize::MAX) {
        return Err(EngineError::InvalidClusterInput);
    }
    if eligible_threads > 0 && documents.is_empty() {
        return Err(EngineError::ClusterVectorsUnavailable);
    }
    let complete_coverage = vector_threads == eligible_threads;
    let repository_full_name = documents
        .first()
        .map(|document| document.summary.repository.full_name.clone())
        .unwrap_or_default();
    let (candidates, candidate_edges) = build_cluster_candidates_bounded(
        documents,
        repository_full_name,
        request.options,
        cancellation,
    )
    .await?;
    if cancellation.is_cancelled() {
        return Err(EngineError::ClusteringCancelled);
    }
    let candidate_edges = u64::try_from(candidate_edges)
        .map_err(|_| forgesync_store::error::StoreError::IntegerOutOfRange)?;
    let clusters = candidates
        .into_iter()
        .map(|cluster| ClusterInput {
            representative: cluster.representative,
            title: cluster.title,
            members: cluster
                .members
                .into_iter()
                .map(|member| ClusterMemberInput {
                    thread: member.summary.discussion.id,
                    score_to_representative: member.score_to_representative,
                })
                .collect(),
        })
        .collect();
    let input = ClusterGenerationInput {
        repository,
        endpoint: endpoint.to_owned(),
        model: model.to_owned(),
        recipe: request.recipe,
        complete_coverage,
        eligible_threads,
        vector_threads,
        candidate_edges,
        clusters,
    };
    let generation = archive
        .save_clusters_fenced(lease, &input, now_utc()?)
        .await?;
    Ok(ClusterBuildReport {
        generation,
        eligible_threads,
        vector_threads,
        candidate_edges,
    })
}

async fn count_open_threads(
    archive: &Archive,
    repositories: &[RepositoryId],
    cancellation: &CancellationToken,
) -> Result<u64, EngineError> {
    let mut offset = 0_u64;
    let mut total = 0_u64;
    loop {
        if cancellation.is_cancelled() {
            return Err(EngineError::ClusteringCancelled);
        }
        let page = archive
            .query_threads(&ThreadQuery {
                repositories: repositories.to_vec(),
                kind: None,
                state: ThreadStateFilter::Open,
                match_expression: None,
                updated_since: None,
                sort: ThreadSort::Created,
                limit: std::num::NonZeroU32::new(CLUSTER_PAGE_SIZE)
                    .expect("cluster page size is non-zero"),
                offset,
            })
            .await?;
        total = total
            .checked_add(
                u64::try_from(page.items.len())
                    .map_err(|_| forgesync_store::error::StoreError::IntegerOutOfRange)?,
            )
            .ok_or(forgesync_store::error::StoreError::IntegerOutOfRange)?;
        let Some(next_offset) = page.next_offset else {
            return Ok(total);
        };
        offset = next_offset;
    }
}

async fn load_cluster_vectors(
    archive: &Archive,
    repositories: &[RepositoryId],
    endpoint: &str,
    model: &str,
    recipe: DocumentRecipe,
    cancellation: &CancellationToken,
) -> Result<Vec<EmbeddingSearchDocument>, EngineError> {
    let limit =
        std::num::NonZeroU32::new(CLUSTER_PAGE_SIZE).expect("cluster page size is non-zero");
    let mut after_document_id = None;
    let mut documents = Vec::new();
    loop {
        if cancellation.is_cancelled() {
            return Err(EngineError::ClusteringCancelled);
        }
        let page = archive
            .embedding_search_page(&EmbeddingDocumentQuery {
                repositories,
                kind: None,
                state: ThreadStateFilter::Open,
                endpoint,
                model,
                recipe,
                after_document_id,
                limit,
            })
            .await?;
        documents.extend(page.items);
        let Some(next_document_id) = page.next_document_id else {
            return Ok(documents);
        };
        after_document_id = Some(next_document_id);
    }
}

async fn build_cluster_candidates_bounded(
    documents: Vec<EmbeddingSearchDocument>,
    repository_full_name: String,
    options: ClusterOptions,
    cancellation: &CancellationToken,
) -> Result<(Vec<ClusterCandidate>, usize), EngineError> {
    let slots = Arc::clone(
        CLUSTER_WORKER_SLOTS.get_or_init(|| Arc::new(Semaphore::new(CLUSTER_WORKER_LIMIT))),
    );
    let permit = tokio::select! {
        _ = cancellation.cancelled() => return Err(EngineError::ClusteringCancelled),
        permit = slots.acquire_owned() => permit.map_err(|_| EngineError::ClusterWorkerFailed)?,
    };
    let worker_cancellation = cancellation.clone();
    tokio::task::spawn_blocking(move || {
        let _permit: OwnedSemaphorePermit = permit;
        build_cluster_candidates(
            documents,
            &repository_full_name,
            options,
            &worker_cancellation,
        )
    })
    .await
    .map_err(|_| EngineError::ClusterWorkerFailed)?
}

async fn finish_cluster_lease<T>(
    archive: &Archive,
    lease: &ArchiveLeaseToken,
    operation: Result<T, forgesync_store::error::StoreError>,
) -> Result<T, EngineError> {
    finish_cluster_lease_result(archive, lease, operation.map_err(Into::into)).await
}

async fn finish_cluster_decision_lease<T>(
    archive: &Archive,
    lease: &ArchiveLeaseToken,
    operation: Result<T, forgesync_store::error::StoreError>,
) -> Result<T, EngineError> {
    let operation = operation.map_err(|error| match error {
        forgesync_store::error::StoreError::ClusterMemberMissing => {
            EngineError::InvalidClusterDecision
        }
        error => error.into(),
    });
    finish_cluster_lease_result(archive, lease, operation).await
}

async fn finish_cluster_lease_result<T>(
    archive: &Archive,
    lease: &ArchiveLeaseToken,
    operation: Result<T, EngineError>,
) -> Result<T, EngineError> {
    let release = match now_utc() {
        Ok(at) => archive
            .release_archive_lease(lease, at)
            .await
            .map_err(EngineError::from)
            .and_then(|released| {
                if released {
                    Ok(())
                } else {
                    Err(forgesync_store::error::StoreError::ArchiveLeaseLost.into())
                }
            }),
        Err(error) => Err(error),
    };
    match (operation, release) {
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Ok(value), Ok(())) => Ok(value),
    }
}

fn offer_neighbor(heap: &mut BinaryHeap<Neighbor>, candidate: Neighbor, capacity: usize) {
    if heap.len() < capacity {
        heap.push(candidate);
        return;
    }
    let Some(worst) = heap.peek() else {
        return;
    };
    let is_better = candidate.score > worst.score
        || (candidate.score.total_cmp(&worst.score) == Ordering::Equal
            && candidate.node_index < worst.node_index);
    if is_better {
        heap.pop();
        heap.push(candidate);
    }
}

fn document_similarity(
    left: &EmbeddingSearchDocument,
    right: &EmbeddingSearchDocument,
) -> Option<f64> {
    left.chunks
        .iter()
        .flat_map(|left_chunk| {
            right.chunks.iter().filter_map(move |right_chunk| {
                cosine_similarity(&left_chunk.vector, &right_chunk.vector)
            })
        })
        .filter(|score| score.is_finite())
        .max_by(f64::total_cmp)
}

fn deterministic_reference_edges(
    documents: &[EmbeddingSearchDocument],
    repository_full_name: &str,
) -> HashMap<(usize, usize), f64> {
    let by_number = documents
        .iter()
        .enumerate()
        .map(|(index, document)| (document.summary.discussion.id.number().get(), index))
        .collect::<HashMap<_, _>>();
    let titles = documents
        .iter()
        .map(|document| title_tokens(&document.summary.discussion.title))
        .collect::<Vec<_>>();
    let mut edges = HashMap::new();
    for (source_index, document) in documents.iter().enumerate() {
        let discussion = &document.summary.discussion;
        collect_reference_edges(
            &mut edges,
            source_index,
            &discussion.title,
            true,
            repository_full_name,
            discussion.id.number().get(),
            &by_number,
            &titles,
        );
        if let Some(body) = discussion.body.as_deref() {
            collect_reference_edges(
                &mut edges,
                source_index,
                body,
                false,
                repository_full_name,
                discussion.id.number().get(),
                &by_number,
                &titles,
            );
        }
    }
    edges
}

#[allow(clippy::too_many_arguments)]
fn collect_reference_edges(
    edges: &mut HashMap<(usize, usize), f64>,
    source_index: usize,
    text: &str,
    is_title: bool,
    repository_full_name: &str,
    source_number: u64,
    by_number: &HashMap<u64, usize>,
    titles: &[HashSet<String>],
) {
    for captures in THREAD_REFERENCE.captures_iter(text) {
        let referenced_repository = captures.get(1).or_else(|| captures.get(3));
        if referenced_repository.is_some_and(|repository| {
            !repository
                .as_str()
                .eq_ignore_ascii_case(repository_full_name)
        }) {
            continue;
        }
        let number_capture = captures
            .get(2)
            .or_else(|| captures.get(4))
            .or_else(|| captures.get(5));
        let Some(number) = number_capture.and_then(|value| value.as_str().parse::<u64>().ok())
        else {
            continue;
        };
        if number == source_number {
            continue;
        }
        let Some(&target_index) = by_number.get(&number) else {
            continue;
        };
        let early_body = !is_title
            && captures
                .get(0)
                .is_some_and(|reference| reference.start() <= EARLY_BODY_REFERENCE_BYTES);
        if !is_title
            && !early_body
            && overlap_ratio(&titles[source_index], &titles[target_index]) < MIN_TITLE_OVERLAP
        {
            continue;
        }
        edges
            .entry((
                source_index.min(target_index),
                source_index.max(target_index),
            ))
            .and_modify(|score| *score = score.max(REFERENCE_SCORE))
            .or_insert(REFERENCE_SCORE);
    }
}

fn title_tokens(value: &str) -> HashSet<String> {
    TITLE_TOKEN
        .find_iter(value)
        .map(|token| token.as_str().to_ascii_lowercase())
        .collect()
}

fn overlap_ratio(left: &HashSet<String>, right: &HashSet<String>) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let overlap = left.intersection(right).count();
    overlap as f64 / left.len().min(right.len()) as f64
}

fn compare_edges(left: &CandidateEdge, right: &CandidateEdge) -> Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.left.cmp(&right.left))
        .then_with(|| left.right.cmp(&right.right))
}

fn bounded_components(
    documents: &[EmbeddingSearchDocument],
    edges: &[CandidateEdge],
    options: ClusterOptions,
) -> (Vec<Vec<usize>>, Vec<CandidateEdge>) {
    let mut groups = UnionFind::new(documents.len());
    let mut kept_edges = Vec::with_capacity(edges.len());
    for edge in edges {
        if groups.union(edge.left, edge.right, options.max_cluster_size) {
            kept_edges.push(*edge);
        }
    }
    let mut components = HashMap::<usize, Vec<usize>>::new();
    for index in 0..documents.len() {
        let root = groups.find(index);
        components.entry(root).or_default().push(index);
    }
    let mut components = components.into_values().collect::<Vec<_>>();
    for members in &mut components {
        members.sort_by(|left, right| {
            stable_thread_id_cmp(&documents[*left].summary, &documents[*right].summary)
        });
    }
    components.sort_by(|left, right| {
        right.len().cmp(&left.len()).then_with(|| {
            stable_thread_id_cmp(&documents[left[0]].summary, &documents[right[0]].summary)
        })
    });
    (components, kept_edges)
}

fn format_clusters(
    documents: &[EmbeddingSearchDocument],
    components: &[Vec<usize>],
    edges: &[CandidateEdge],
    min_size: usize,
) -> Vec<ClusterCandidate> {
    let mut degrees = vec![0_usize; documents.len()];
    let mut edge_scores = HashMap::with_capacity(edges.len());
    for edge in edges {
        degrees[edge.left] = degrees[edge.left].saturating_add(1);
        degrees[edge.right] = degrees[edge.right].saturating_add(1);
        edge_scores.insert(
            (edge.left.min(edge.right), edge.left.max(edge.right)),
            edge.score,
        );
    }
    components
        .iter()
        .filter(|members| members.len() >= min_size)
        .map(|members| {
            let representative = members
                .iter()
                .copied()
                .min_by(|left, right| {
                    degrees[*right]
                        .cmp(&degrees[*left])
                        .then_with(|| {
                            documents[*left]
                                .summary
                                .discussion
                                .id
                                .number()
                                .cmp(&documents[*right].summary.discussion.id.number())
                        })
                        .then_with(|| {
                            stable_thread_id_cmp(
                                &documents[*left].summary,
                                &documents[*right].summary,
                            )
                        })
                })
                .expect("a component has at least one member");
            let representative_id = documents[representative].summary.discussion.id.clone();
            let title = documents[representative].summary.discussion.title.clone();
            let members = members
                .iter()
                .map(|member| ClusterMemberCandidate {
                    summary: documents[*member].summary.clone(),
                    score_to_representative: if *member == representative {
                        Some(1.0)
                    } else {
                        edge_scores
                            .get(&((*member).min(representative), (*member).max(representative)))
                            .copied()
                    },
                })
                .collect();
            ClusterCandidate {
                representative: representative_id,
                title,
                members,
            }
        })
        .collect()
}

struct UnionFind {
    parent: Vec<usize>,
    size: Vec<usize>,
}

impl UnionFind {
    fn new(count: usize) -> Self {
        Self {
            parent: (0..count).collect(),
            size: vec![1; count],
        }
    }

    fn find(&mut self, index: usize) -> usize {
        if self.parent[index] != index {
            self.parent[index] = self.find(self.parent[index]);
        }
        self.parent[index]
    }

    fn union(&mut self, left: usize, right: usize, max_size: usize) -> bool {
        let mut left_root = self.find(left);
        let mut right_root = self.find(right);
        if left_root == right_root {
            return true;
        }
        if self.size[left_root] < self.size[right_root] {
            std::mem::swap(&mut left_root, &mut right_root);
        }
        if self.size[left_root].saturating_add(self.size[right_root]) > max_size {
            return false;
        }
        self.parent[right_root] = left_root;
        self.size[left_root] += self.size[right_root];
        true
    }
}

fn max_score(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(score), None) | (None, Some(score)) => Some(score),
        (None, None) => None,
    }
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
    use forgesync_store::reads::ThreadSummary;
    use tokio_util::sync::CancellationToken;

    use super::{ClusterOptions, build_cluster_candidates};

    #[test]
    fn cluster_graph_applies_weak_title_and_cross_kind_safeguards() {
        let docs = vec![
            document(
                1,
                ThreadKind::Issue,
                "Cache eviction memory crash",
                None,
                &[1.0, 0.0],
            ),
            document(
                2,
                ThreadKind::Issue,
                "Memory crash after eviction",
                None,
                &[0.85, 0.5267827],
            ),
            document(
                3,
                ThreadKind::Issue,
                "Unrelated clipboard outage",
                None,
                &[0.0, 1.0],
            ),
            document(
                4,
                ThreadKind::PullRequest,
                "Cache eviction memory crash",
                None,
                &[0.92, -0.39191836],
            ),
            document(
                5,
                ThreadKind::PullRequest,
                "Cache eviction memory crash",
                None,
                &[0.95, 0.3122499],
            ),
        ];
        let (clusters, edge_count) = build_cluster_candidates(
            docs,
            "example/repo",
            ClusterOptions {
                threshold: 0.80,
                cross_kind_threshold: 0.93,
                fanout: 16,
                max_cluster_size: 40,
                min_cluster_size: 1,
            },
            &CancellationToken::new(),
        )
        .expect("build graph");
        assert_eq!(edge_count, 3);
        assert_eq!(
            clusters
                .iter()
                .map(|cluster| cluster.members.len())
                .collect::<Vec<_>>(),
            [3, 1, 1]
        );
        assert_eq!(clusters[0].representative.number().get(), 1);
    }

    #[test]
    fn references_are_repository_scoped_and_early_body_evidence_is_strong() {
        let docs = vec![
            document(
                101,
                ThreadKind::Issue,
                "Token expires too early",
                None,
                &[1.0, 0.0],
            ),
            document(
                102,
                ThreadKind::PullRequest,
                "Unrelated patch",
                Some("See #101 for the report"),
                &[0.0, 1.0],
            ),
            document(
                103,
                ThreadKind::Issue,
                "Different repository",
                Some("example/other#101"),
                &[0.0, -1.0],
            ),
        ];
        let (clusters, edge_count) = build_cluster_candidates(
            docs,
            "example/repo",
            ClusterOptions::default(),
            &CancellationToken::new(),
        )
        .expect("build graph");
        assert_eq!(edge_count, 1, "clusters: {clusters:#?}");
        let referenced = clusters
            .iter()
            .find(|cluster| cluster.members.len() == 2)
            .expect("reference cluster");
        assert_eq!(referenced.members[0].score_to_representative, Some(1.0));
    }

    #[test]
    fn fanout_and_maximum_size_keep_deterministic_components() {
        let docs = (1..=6)
            .map(|number| {
                document(
                    number,
                    ThreadKind::Issue,
                    "Shared memory issue",
                    None,
                    &[1.0, number as f32 / 100.0],
                )
            })
            .collect::<Vec<_>>();
        let (first, _) = build_cluster_candidates(
            docs.clone(),
            "example/repo",
            ClusterOptions {
                fanout: 1,
                max_cluster_size: 3,
                ..ClusterOptions::default()
            },
            &CancellationToken::new(),
        )
        .expect("first graph");
        let (second, _) = build_cluster_candidates(
            docs,
            "example/repo",
            ClusterOptions {
                fanout: 1,
                max_cluster_size: 3,
                ..ClusterOptions::default()
            },
            &CancellationToken::new(),
        )
        .expect("second graph");
        let memberships = |clusters: &[super::ClusterCandidate]| {
            clusters
                .iter()
                .map(|cluster| {
                    cluster
                        .members
                        .iter()
                        .map(|member| member.summary.discussion.id.number().get())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(memberships(&first), memberships(&second));
        assert!(first.iter().all(|cluster| cluster.members.len() <= 3));
    }

    #[test]
    fn cancellation_stops_graph_construction() {
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let error = build_cluster_candidates(
            vec![document(1, ThreadKind::Issue, "One", None, &[1.0, 0.0])],
            "example/repo",
            ClusterOptions::default(),
            &cancellation,
        )
        .expect_err("cancelled graph");
        assert_eq!(error.code(), "operation_cancelled");
    }

    fn document(
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
}
