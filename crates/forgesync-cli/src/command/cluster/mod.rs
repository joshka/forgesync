//! Cluster generation, inspection, and local maintainer decisions.
//!
//! Generated proposals and maintainer decisions have different authorship and persistence rules,
//! owned by the engine and store. Decisions are recorded locally and never written to GitHub.

use std::num::NonZeroU64;
use std::path::Path;

use clap::builder::RangedU64ValueParser;
use clap::{ArgAction, Args, Subcommand};
use forgesync_engine::clustering::{
    ClusterListRequest, dismiss_cluster, exclude_cluster_member, include_cluster_member,
    list_clusters, restore_cluster, set_canonical_cluster_member, show_cluster,
};
use forgesync_engine::error::EngineError;
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};
use forgesync_store::archive::Archive;
use tokio_util::sync::CancellationToken;

use super::embed::EmbeddingIdentityArgs;
use super::with_archive;
use crate::config::ForgesyncConfig;
use crate::error::{CliError, Exit};
use crate::output::Output;
use crate::reports::clusters::{
    ClusterDecisionOutput, cluster_decision_summary, cluster_detail_summary, cluster_page_summary,
};

mod build;

/// Deterministic cluster generation, inspection, and local governance operations.
#[derive(Clone, Debug, Subcommand)]
pub enum ClusterCommand {
    /// Build groups from current open discussions and compatible stored vectors.
    Build(ClusterBuildArgs),
    /// List current clusters, optionally including those retired by a complete run.
    List(ClusterListArgs),
    /// Show one generated cluster and its current member decisions.
    Show {
        /// Positive archive-local cluster ID.
        id: NonZeroU64,
    },
    /// Hide one generated cluster from local triage without changing GitHub.
    Dismiss {
        /// Positive archive-local cluster ID.
        id: NonZeroU64,
        /// Optional maintainer rationale retained in the archive.
        #[arg(long)]
        reason: Option<String>,
    },
    /// Clear a local cluster dismissal.
    Restore {
        /// Positive archive-local cluster ID.
        id: NonZeroU64,
    },
    /// Exclude one current member from a generated cluster.
    Exclude {
        /// Positive archive-local cluster ID.
        id: NonZeroU64,
        /// OWNER/REPO#NUMBER or a GitHub issue/pull-request URL.
        member: ThreadSelector,
        /// Optional maintainer rationale retained in the archive.
        #[arg(long)]
        reason: Option<String>,
    },
    /// Include a previously excluded current member.
    Include {
        /// Positive archive-local cluster ID.
        id: NonZeroU64,
        /// OWNER/REPO#NUMBER or a GitHub issue/pull-request URL.
        member: ThreadSelector,
    },
    /// Select the canonical discussion for one generated cluster.
    Canonical {
        /// Positive archive-local cluster ID.
        id: NonZeroU64,
        /// OWNER/REPO#NUMBER or a GitHub issue/pull-request URL.
        member: ThreadSelector,
    },
}

/// Accepts finite inclusive thresholds so clustering never receives NaN or an invalid range.
fn parse_unit_float(value: &str) -> Result<f64, String> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| "value must be a number between 0 and 1".to_owned())?;
    if !parsed.is_finite() || !(0.0..=1.0).contains(&parsed) {
        return Err("value must be between 0 and 1".to_owned());
    }
    Ok(parsed)
}

/// Inputs for one deterministic cluster generation.
#[derive(Clone, Debug, Args)]
pub struct ClusterBuildArgs {
    /// Registered repository to cluster.
    #[arg(value_name = "OWNER/REPO")]
    pub repository: RepositorySelector,
    /// Select stored vectors by embedding service identity; no request is sent.
    #[command(flatten)]
    pub service: EmbeddingIdentityArgs,
    /// Minimum cosine similarity for a same-kind edge.
    #[arg(long, default_value_t = 0.80, value_parser = parse_unit_float)]
    pub threshold: f64,
    /// Minimum cosine similarity for an issue-to-pull-request edge.
    #[arg(long, default_value_t = 0.93, value_parser = parse_unit_float)]
    pub cross_kind_threshold: f64,
    /// Maximum retained neighbors per discussion (1-256).
    #[arg(long, default_value_t = 16, value_parser = RangedU64ValueParser::<usize>::new().range(1..=256))]
    pub fanout: usize,
    /// Maximum component size (1-10000).
    #[arg(long, default_value_t = 40, value_parser = RangedU64ValueParser::<usize>::new().range(1..=10000))]
    pub max_cluster_size: usize,
    /// Minimum component size to persist (1-10000).
    #[arg(long, default_value_t = 1, value_parser = RangedU64ValueParser::<usize>::new().range(1..=10000))]
    pub min_cluster_size: usize,
}

/// Filters for one cluster listing.
#[derive(Clone, Debug, Args)]
pub struct ClusterListArgs {
    /// Limit results to registered repositories.
    #[arg(long = "repo", value_name = "OWNER/REPO")]
    pub repositories: Vec<RepositorySelector>,
    /// Include retired clusters.
    #[arg(long, action = ArgAction::SetTrue)]
    pub include_retired: bool,
    /// Maximum number of clusters (1-1000).
    #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u32).range(1..=1000))]
    pub limit: u32,
    /// Number of clusters to skip.
    #[arg(long, default_value_t = 0)]
    pub offset: u64,
}

impl ClusterCommand {
    /// Runs a cluster build, read, or local decision against the archive.
    pub async fn run(
        self,
        path: &Path,
        output: Output,
        verbose: u8,
        config: ForgesyncConfig,
        cancellation: &CancellationToken,
    ) -> Result<Exit, CliError> {
        match self {
            Self::Build(args) => args.run(path, output, verbose, config, cancellation).await,
            Self::List(args) => {
                let request = ClusterListRequest {
                    repositories: args.repositories,
                    include_retired: args.include_retired,
                    limit: args.limit,
                    offset: args.offset,
                };
                let page = with_archive(Archive::open_read_only(path), async |archive| {
                    list_clusters(archive, &request).await
                })
                .await?;
                Ok(output.success(&page, cluster_page_summary))
            }
            Self::Show { id } => {
                let detail = with_archive(Archive::open_read_only(path), async |archive| {
                    show_cluster(archive, id.get()).await
                })
                .await?;
                Ok(output.success(&detail, cluster_detail_summary))
            }
            Self::Dismiss { id, reason } => {
                let reason = reason.unwrap_or_default();
                decide(path, output, id, "dismissed", async |archive| {
                    dismiss_cluster(archive, id.get(), &reason).await
                })
                .await
            }
            Self::Restore { id } => {
                decide(path, output, id, "restored", async |archive| {
                    restore_cluster(archive, id.get()).await
                })
                .await
            }
            Self::Exclude { id, member, reason } => {
                let reason = reason.unwrap_or_default();
                decide(path, output, id, "member_excluded", async |archive| {
                    exclude_cluster_member(archive, id.get(), &member, &reason).await
                })
                .await
            }
            Self::Include { id, member } => {
                decide(path, output, id, "member_included", async |archive| {
                    include_cluster_member(archive, id.get(), &member).await
                })
                .await
            }
            Self::Canonical { id, member } => {
                decide(path, output, id, "canonical_set", async |archive| {
                    set_canonical_cluster_member(archive, id.get(), &member).await
                })
                .await
            }
        }
    }
}

/// Records one local maintainer decision and acknowledges the action on success.
async fn decide(
    path: &Path,
    output: Output,
    id: NonZeroU64,
    action: &'static str,
    operation: impl AsyncFnOnce(&Archive) -> Result<(), EngineError>,
) -> Result<Exit, CliError> {
    with_archive(Archive::open_read_write(path), operation).await?;
    let acknowledgment = ClusterDecisionOutput {
        cluster_id: id.get(),
        action,
    };
    Ok(output.success(&acknowledgment, cluster_decision_summary))
}
