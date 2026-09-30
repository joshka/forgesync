//! # List and show stored clusters
//!
//! [`ClusterListArgs::run_list`] translates CLI filters into an engine cluster-list request.
//! [`run_show`] requests one persisted cluster by its archive-local ID. A list page is a navigation
//! summary; detail exposes members and their current local triage state. Neither operation rebuilds
//! candidates, changes decisions, prepares credentials, or contacts a provider.
//!
//! Both handlers open an existing archive read-only, obtain an engine projection, and close the
//! handle before rendering either data or a typed failure. Opening does not create or migrate the
//! database. The engine and store own selection and identity checks; these handlers own only the
//! argument-to-request boundary and process presentation.
//!
//! Retired-cluster inclusion is an explicit list choice. Persisted results describe the stored
//! generation and decisions, rather than certifying that source evidence or embeddings are current.
//! Human layouts come from the cluster report module, while JSON uses the shared output envelope.

use std::path::Path;
use std::process::ExitCode;

use forgesync_engine::clustering::{ClusterListRequest, list_clusters, show_cluster};
use forgesync_store::archive::Archive;

use crate::command::cluster::ClusterListArgs;
use crate::reports::clusters::{cluster_detail_summary, cluster_page_summary};
use crate::{OutputMode, render_engine_error, render_store_error, render_success};

impl ClusterListArgs {
    /// Lists stored generations with an explicit engine request and read-only archive lifetime.
    ///
    /// Conversion preserves parsed filters without resolving repositories. The archive closes
    /// before either the page or typed engine error is presented.
    pub async fn run_list(self, path: &Path, json: OutputMode) -> ExitCode {
        let request = self.into_request();
        let archive = match Archive::open_read_only(path).await {
            Ok(archive) => archive,
            Err(error) => return render_store_error(json, "cluster list", error),
        };
        let result = list_clusters(&archive, &request).await;
        archive.close().await;
        match result {
            Ok(page) => render_success(json, "cluster list", &page, cluster_page_summary),
            Err(error) => render_engine_error(json, "cluster list", error),
        }
    }

    /// Transfers repository, retirement, and pagination choices into the engine's read request.
    ///
    /// This conversion performs no archive access and retains the engine's identity resolution
    /// and filtering responsibility.
    fn into_request(self) -> ClusterListRequest {
        ClusterListRequest {
            repositories: self.repositories,
            include_retired: self.include_retired,
            limit: self.limit,
            offset: self.offset,
        }
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
