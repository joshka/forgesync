//! # Record explicit local cluster decisions
//!
//! Dismiss, restore, exclude, include, and canonical-selection handlers adapt parsed CLI targets
//! into engine decision operations. Their cluster ID refers to an existing archive record; member
//! selectors are resolved against local data by the engine. Optional reasons become an empty
//! reason when omitted. These handlers neither acquire source evidence nor construct candidates.
//!
//! Each operation opens the existing archive for writes, invokes its named engine action, and
//! closes the handle before presenting the result. The engine and store own target validation,
//! write authority, transactions, and durable decision semantics. No handler writes to GitHub.
//!
//! The shared rendering helper receives the finished result and emits a small acknowledgment
//! containing the cluster ID and action name. It does not reload cluster detail or make an
//! optimistic state change. Store failures keep store diagnostic codes; other engine failures keep
//! engine codes. A rendering failure after a successful operation does not undo its durable write.
//!
//! Keeping these adapters separate from generation makes maintainer authorship explicit: analysis
//! proposes groups, while these entry points record the user's choice about a stored group.

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
    let archive = match Archive::open_read_write(path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(json, "cluster dismiss", error),
    };
    let result = dismiss_cluster(&archive, id, reason.as_deref().unwrap_or("")).await;
    archive.close().await;
    let output = ClusterDecisionOutput {
        cluster_id: id,
        action: "dismissed",
    };
    render_decision(json, "cluster dismiss", output, result)
}

/// Restores a locally dismissed cluster to maintainer triage.
pub async fn run_restore(id: u64, path: &Path, json: OutputMode) -> ExitCode {
    let archive = match Archive::open_read_write(path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(json, "cluster restore", error),
    };
    let result = restore_cluster(&archive, id).await;
    archive.close().await;
    let output = ClusterDecisionOutput {
        cluster_id: id,
        action: "restored",
    };
    render_decision(json, "cluster restore", output, result)
}

/// Excludes a member from local cluster triage and retains an optional reason.
pub async fn run_exclude(
    id: u64,
    member: ThreadSelector,
    reason: Option<String>,
    path: &Path,
    json: OutputMode,
) -> ExitCode {
    let archive = match Archive::open_read_write(path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(json, "cluster exclude", error),
    };
    let result =
        exclude_cluster_member(&archive, id, &member, reason.as_deref().unwrap_or("")).await;
    archive.close().await;
    let output = ClusterDecisionOutput {
        cluster_id: id,
        action: "member_excluded",
    };
    render_decision(json, "cluster exclude", output, result)
}

/// Returns a previously excluded member to its generated cluster.
pub async fn run_include(
    id: u64,
    member: ThreadSelector,
    path: &Path,
    json: OutputMode,
) -> ExitCode {
    let archive = match Archive::open_read_write(path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(json, "cluster include", error),
    };
    let result = include_cluster_member(&archive, id, &member).await;
    archive.close().await;
    let output = ClusterDecisionOutput {
        cluster_id: id,
        action: "member_included",
    };
    render_decision(json, "cluster include", output, result)
}

/// Records the canonical discussion selected by the maintainer.
pub async fn run_set_canonical(
    id: u64,
    member: ThreadSelector,
    path: &Path,
    json: OutputMode,
) -> ExitCode {
    let archive = match Archive::open_read_write(path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(json, "cluster canonical", error),
    };
    let result = set_canonical_cluster_member(&archive, id, &member).await;
    archive.close().await;
    let output = ClusterDecisionOutput {
        cluster_id: id,
        action: "canonical_set",
    };
    render_decision(json, "cluster canonical", output, result)
}

/// Presents an already finished local decision with its typed engine or store failure.
///
/// The handler has closed the archive before calling this renderer. The acknowledgment describes
/// the selected action only on success; rendering never invokes or retries the mutation.
fn render_decision(
    json: OutputMode,
    command: &'static str,
    output: ClusterDecisionOutput,
    result: Result<(), EngineError>,
) -> ExitCode {
    match result {
        Ok(()) => render_success(json, command, &output, cluster_decision_summary),
        Err(EngineError::Store(error)) => render_store_error(json, command, error),
        Err(error) => render_engine_error(json, command, error),
    }
}
