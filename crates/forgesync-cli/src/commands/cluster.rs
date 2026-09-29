//! Cluster command handling.

use super::*;

pub(super) struct ClusterCliRequest<'a> {
    pub(super) archive_path: &'a std::path::Path,
    pub(super) command: ClusterCommand,
    pub(super) embedding_service: crate::config::EmbeddingServiceConfig,
    pub(super) recipe: DocumentRecipe,
    pub(super) json: bool,
    pub(super) verbose: u8,
}

pub(super) async fn cluster_from_cli(request: ClusterCliRequest<'_>) -> ExitCode {
    let cancellation = tokio_util::sync::CancellationToken::new();
    let interrupt_cancellation = cancellation.clone();
    let interrupt_task = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            interrupt_cancellation.cancel();
        }
    });
    let result = cluster_command(
        request.archive_path,
        request.command,
        request.embedding_service,
        request.recipe,
        request.json,
        request.verbose,
        &cancellation,
    )
    .await;
    interrupt_task.abort();
    result
}

pub(super) async fn cluster_command(
    archive_path: &std::path::Path,
    command: ClusterCommand,
    mut embedding_service: crate::config::EmbeddingServiceConfig,
    recipe: DocumentRecipe,
    json: bool,
    verbose: u8,
    cancellation: &tokio_util::sync::CancellationToken,
) -> ExitCode {
    match command {
        ClusterCommand::Build {
            repository,
            endpoint,
            model,
            threshold,
            cross_kind_threshold,
            fanout,
            max_cluster_size,
            min_cluster_size,
        } => {
            if let Some(endpoint) = endpoint {
                embedding_service.endpoint = endpoint;
            }
            if let Some(model) = model {
                embedding_service.model = model;
            }
            if let Err(error) = embedding_service.validate() {
                return render_error_with_status(
                    json,
                    "cluster build",
                    error.code(),
                    &error.to_string(),
                    ExitCode::from(2),
                );
            }
            let endpoint = match url::Url::parse(&embedding_service.endpoint) {
                Ok(endpoint) => endpoint.as_str().trim_end_matches('/').to_owned(),
                Err(_) => {
                    return render_error_with_status(
                        json,
                        "cluster build",
                        "embedding_configuration_invalid",
                        "embedding service configuration is invalid",
                        ExitCode::from(2),
                    );
                }
            };
            let request = ClusterBuildRequest {
                repository,
                endpoint,
                model: embedding_service.model.trim().to_owned(),
                recipe,
                options: ClusterOptions {
                    threshold,
                    cross_kind_threshold,
                    fanout: usize::try_from(fanout).unwrap_or(usize::MAX),
                    max_cluster_size: usize::try_from(max_cluster_size).unwrap_or(usize::MAX),
                    min_cluster_size: usize::try_from(min_cluster_size).unwrap_or(usize::MAX),
                },
            };
            if verbose > 0 && !json {
                eprintln!(
                    "forgesync: building local clusters for {} using stored vectors",
                    request.repository.as_url()
                );
            }
            let archive = match Archive::open_read_write(archive_path).await {
                Ok(archive) => archive,
                Err(error) => return render_store_error(json, "cluster build", error),
            };
            let result = build_clusters(&archive, &request, cancellation).await;
            archive.close().await;
            match result {
                Ok(report) => {
                    let status = if report.generation.complete_coverage {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::from(3)
                    };
                    render_result(
                        json,
                        "cluster build",
                        &report,
                        cluster_build_summary,
                        status,
                    )
                }
                Err(error) => render_engine_error(json, "cluster build", error),
            }
        }
        ClusterCommand::List {
            repositories,
            include_retired,
            limit,
            offset,
        } => match Archive::open_read_only(archive_path).await {
            Ok(archive) => {
                let result = list_clusters(
                    &archive,
                    &ClusterListRequest {
                        repositories,
                        include_retired,
                        limit,
                        offset,
                    },
                )
                .await;
                archive.close().await;
                match result {
                    Ok(page) => render_success(json, "cluster list", &page, cluster_page_summary),
                    Err(error) => render_engine_error(json, "cluster list", error),
                }
            }
            Err(error) => render_store_error(json, "cluster list", error),
        },
        ClusterCommand::Show { id } => match Archive::open_read_only(archive_path).await {
            Ok(archive) => {
                let result = show_cluster(&archive, id).await;
                archive.close().await;
                match result {
                    Ok(detail) => {
                        render_success(json, "cluster show", &detail, cluster_detail_summary)
                    }
                    Err(error) => render_engine_error(json, "cluster show", error),
                }
            }
            Err(error) => render_store_error(json, "cluster show", error),
        },
        ClusterCommand::Dismiss { id, reason } => {
            mutate_cluster(json, "cluster dismiss", id, "dismissed", async {
                let archive = Archive::open_read_write(archive_path).await?;
                let result = dismiss_cluster(&archive, id, reason.as_deref().unwrap_or("")).await;
                archive.close().await;
                result
            })
            .await
        }
        ClusterCommand::Restore { id } => {
            mutate_cluster(json, "cluster restore", id, "restored", async {
                let archive = Archive::open_read_write(archive_path).await?;
                let result = restore_cluster(&archive, id).await;
                archive.close().await;
                result
            })
            .await
        }
        ClusterCommand::Exclude { id, member, reason } => {
            mutate_cluster(json, "cluster exclude", id, "member_excluded", async move {
                let archive = Archive::open_read_write(archive_path).await?;
                let result =
                    exclude_cluster_member(&archive, id, &member, reason.as_deref().unwrap_or(""))
                        .await;
                archive.close().await;
                result
            })
            .await
        }
        ClusterCommand::Include { id, member } => {
            mutate_cluster(json, "cluster include", id, "member_included", async move {
                let archive = Archive::open_read_write(archive_path).await?;
                let result = include_cluster_member(&archive, id, &member).await;
                archive.close().await;
                result
            })
            .await
        }
        ClusterCommand::Canonical { id, member } => {
            mutate_cluster(json, "cluster canonical", id, "canonical_set", async move {
                let archive = Archive::open_read_write(archive_path).await?;
                let result = set_canonical_cluster_member(&archive, id, &member).await;
                archive.close().await;
                result
            })
            .await
        }
    }
}

pub(super) async fn mutate_cluster<F>(
    json: bool,
    command: &'static str,
    cluster_id: u64,
    action: &'static str,
    operation: F,
) -> ExitCode
where
    F: std::future::Future<Output = Result<(), EngineError>>,
{
    match operation.await {
        Ok(()) => render_success(
            json,
            command,
            &ClusterDecisionOutput { cluster_id, action },
            cluster_decision_summary,
        ),
        Err(EngineError::Store(error)) => render_store_error(json, command, error),
        Err(error) => render_engine_error(json, command, error),
    }
}
