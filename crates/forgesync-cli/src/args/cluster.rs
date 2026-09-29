//! Cluster command arguments.

use clap::{ArgAction, Args, Subcommand};
use forgesync_engine::reference::{RepositorySelector, ThreadSelector};

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
        #[arg(value_parser = clap::value_parser!(u64).range(1..))]
        id: u64,
    },
    /// Hide one generated cluster from local triage without changing GitHub.
    Dismiss {
        /// Positive archive-local cluster ID.
        #[arg(value_parser = clap::value_parser!(u64).range(1..))]
        id: u64,
        /// Optional maintainer rationale retained in the archive.
        #[arg(long)]
        reason: Option<String>,
    },
    /// Clear a local cluster dismissal.
    Restore {
        /// Positive archive-local cluster ID.
        #[arg(value_parser = clap::value_parser!(u64).range(1..))]
        id: u64,
    },
    /// Exclude one current member from a generated cluster.
    Exclude {
        /// Positive archive-local cluster ID.
        #[arg(value_parser = clap::value_parser!(u64).range(1..))]
        id: u64,
        /// OWNER/REPO#NUMBER or a GitHub issue/pull-request URL.
        member: ThreadSelector,
        /// Optional maintainer rationale retained in the archive.
        #[arg(long)]
        reason: Option<String>,
    },
    /// Include a previously excluded current member.
    Include {
        /// Positive archive-local cluster ID.
        #[arg(value_parser = clap::value_parser!(u64).range(1..))]
        id: u64,
        /// OWNER/REPO#NUMBER or a GitHub issue/pull-request URL.
        member: ThreadSelector,
    },
    /// Select the canonical discussion for one generated cluster.
    Canonical {
        /// Positive archive-local cluster ID.
        #[arg(value_parser = clap::value_parser!(u64).range(1..))]
        id: u64,
        /// OWNER/REPO#NUMBER or a GitHub issue/pull-request URL.
        member: ThreadSelector,
    },
}

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
    /// Override the configured embedding base endpoint; no request is sent.
    #[arg(long, value_name = "URL")]
    pub endpoint: Option<String>,
    /// Override the configured embedding model identity.
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,
    /// Minimum cosine similarity for a same-kind edge.
    #[arg(
        long,
        default_value_t = 0.80,
        value_parser = parse_unit_float
    )]
    pub threshold: f64,
    /// Minimum cosine similarity for an issue-to-pull-request edge.
    #[arg(
        long,
        default_value_t = 0.93,
        value_parser = parse_unit_float
    )]
    pub cross_kind_threshold: f64,
    /// Maximum retained neighbors per discussion (1-256).
    #[arg(long, default_value_t = 16, value_parser = clap::value_parser!(u32).range(1..=256))]
    pub fanout: u32,
    /// Maximum component size (1-10000).
    #[arg(long, default_value_t = 40, value_parser = clap::value_parser!(u32).range(1..=10000))]
    pub max_cluster_size: u32,
    /// Minimum component size to persist (1-10000).
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=10000))]
    pub min_cluster_size: u32,
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
