//! Build cluster behavior.

use super::*;

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
