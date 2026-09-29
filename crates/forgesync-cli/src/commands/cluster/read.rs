//! Read generated clusters from the local archive.

use std::path::Path;
use std::process::ExitCode;

use forgesync_engine::clustering::{ClusterListRequest, list_clusters, show_cluster};
use forgesync_store::archive::Archive;

use crate::args::ClusterListArgs;
use crate::reports::{cluster_detail_summary, cluster_page_summary};
use crate::{OutputMode, render_engine_error, render_store_error, render_success};

pub(super) async fn list_cluster_command(
    args: ClusterListArgs,
    path: &Path,
    json: OutputMode,
) -> ExitCode {
    let ClusterListArgs {
        repositories,
        include_retired,
        limit,
        offset,
    } = args;
    let archive = match Archive::open_read_only(path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(json, "cluster list", error),
    };
    let request = ClusterListRequest {
        repositories,
        include_retired,
        limit,
        offset,
    };
    let result = list_clusters(&archive, &request).await;
    archive.close().await;
    match result {
        Ok(page) => render_success(json, "cluster list", &page, cluster_page_summary),
        Err(error) => render_engine_error(json, "cluster list", error),
    }
}

pub(super) async fn show_cluster_command(id: u64, path: &Path, json: OutputMode) -> ExitCode {
    let archive = match Archive::open_read_only(path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(json, "cluster show", error),
    };
    let result = show_cluster(&archive, id).await;
    archive.close().await;
    match result {
        Ok(detail) => render_success(json, "cluster show", &detail, cluster_detail_summary),
        Err(error) => render_engine_error(json, "cluster show", error),
    }
}
