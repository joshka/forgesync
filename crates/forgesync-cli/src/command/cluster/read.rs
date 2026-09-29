//! # List and show stored clusters
//!
//! Read handlers open the archive and ask the engine for persisted cluster pages or detail. They
//! format the projection without rebuilding analysis or changing decisions.
//!
//! List and show are separate entry points because a page is a navigation summary while detail
//! includes members and current triage state. Both should remain usable offline.

use std::path::Path;
use std::process::ExitCode;

use forgesync_engine::clustering::{ClusterListRequest, list_clusters, show_cluster};
use forgesync_store::archive::Archive;

use crate::command::cluster::ClusterListArgs;
use crate::reports::clusters::{cluster_detail_summary, cluster_page_summary};
use crate::{OutputMode, render_engine_error, render_store_error, render_success};

/// Lists generated clusters from a read-only archive handle.
pub async fn run_list(args: ClusterListArgs, path: &Path, json: OutputMode) -> ExitCode {
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

/// Shows one cluster and its current local decisions without provider access.
pub async fn run_show(id: u64, path: &Path, json: OutputMode) -> ExitCode {
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
