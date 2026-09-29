//! # Run explicit local cluster decisions
//!
//! Dismiss, restore, exclude, include, and canonical-selection handlers parse the user's target
//! and reason, invoke engine decision operations, and present the result. Each action changes
//! local triage state for a stored cluster.
//!
//! These handlers do not construct cluster candidates. Their input is an existing cluster
//! identity, and their output should make the recorded choice clear in both human and JSON modes.

use std::path::Path;
use std::process::ExitCode;

use forgesync_engine::clustering::{
    dismiss_cluster, exclude_cluster_member, include_cluster_member, restore_cluster,
    set_canonical_cluster_member,
};
use forgesync_engine::error::EngineError;
use forgesync_engine::reference::ThreadSelector;
use forgesync_store::archive::Archive;

use crate::reports::clusters::{ClusterDecisionOutput, cluster_decision_summary};
use crate::{OutputMode, render_engine_error, render_store_error, render_success};

/// Records a local dismissal and optional rationale without changing GitHub.
pub async fn run_dismiss(
    id: u64,
    reason: Option<String>,
    path: &Path,
    json: OutputMode,
) -> ExitCode {
    mutate_cluster(json, "cluster dismiss", id, "dismissed", async {
        let archive = Archive::open_read_write(path).await?;
        let result = dismiss_cluster(&archive, id, reason.as_deref().unwrap_or("")).await;
        archive.close().await;
        result
    })
    .await
}

/// Restores a locally dismissed cluster to maintainer triage.
pub async fn run_restore(id: u64, path: &Path, json: OutputMode) -> ExitCode {
    mutate_cluster(json, "cluster restore", id, "restored", async {
        let archive = Archive::open_read_write(path).await?;
        let result = restore_cluster(&archive, id).await;
        archive.close().await;
        result
    })
    .await
}

/// Excludes a member from local cluster triage and retains an optional reason.
pub async fn run_exclude(
    id: u64,
    member: ThreadSelector,
    reason: Option<String>,
    path: &Path,
    json: OutputMode,
) -> ExitCode {
    mutate_cluster(json, "cluster exclude", id, "member_excluded", async move {
        let archive = Archive::open_read_write(path).await?;
        let result =
            exclude_cluster_member(&archive, id, &member, reason.as_deref().unwrap_or("")).await;
        archive.close().await;
        result
    })
    .await
}

/// Returns a previously excluded member to its generated cluster.
pub async fn run_include(
    id: u64,
    member: ThreadSelector,
    path: &Path,
    json: OutputMode,
) -> ExitCode {
    mutate_cluster(json, "cluster include", id, "member_included", async move {
        let archive = Archive::open_read_write(path).await?;
        let result = include_cluster_member(&archive, id, &member).await;
        archive.close().await;
        result
    })
    .await
}

/// Records the canonical discussion selected by the maintainer.
pub async fn run_set_canonical(
    id: u64,
    member: ThreadSelector,
    path: &Path,
    json: OutputMode,
) -> ExitCode {
    mutate_cluster(json, "cluster canonical", id, "canonical_set", async move {
        let archive = Archive::open_read_write(path).await?;
        let result = set_canonical_cluster_member(&archive, id, &member).await;
        archive.close().await;
        result
    })
    .await
}

/// Renders a local decision consistently while preserving typed engine and store failures.
async fn mutate_cluster<F>(
    json: OutputMode,
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
