//! # Cluster request preparation without acquisition
//!
//! These cases exercise the parsed command's conversion to one engine build request. Endpoint
//! and model overrides identify archived vectors; they do not require credentials or model I/O.
//! Recipe and graph bounds remain explicit parts of the request sent to the engine.
//!
//! A static argument fixture keeps the scenarios independent of Clap parsing, which has its own
//! command tests. Each case spells out the changed inputs and expected request or failure.
//! Invalid service identity fails during preparation before any archive can be opened.
//!
//! Generation integration tests cover persistence and coverage-dependent retirement. Request
//! preparation tests establish identity and policy conversion independently of that larger setup.

use forgesync_core::document::DocumentRecipe;

use crate::command::cluster::ClusterBuildArgs;
use crate::config::{ConfigError, EmbeddingServiceConfig};

#[test]
fn overrides_preserve_canonical_endpoint_and_trimmed_model_identity() {
    let args = ClusterBuildArgs {
        endpoint: Some("https://vectors.example/v1/".to_owned()),
        model: Some("  selected-model  ".to_owned()),
        ..test_args()
    };

    let request = args
        .prepare(
            EmbeddingServiceConfig::default(),
            DocumentRecipe::OriginalBody,
        )
        .expect("prepare request");

    assert_eq!(request.endpoint, "https://vectors.example/v1");
    assert_eq!(request.model, "selected-model");
    assert_eq!(request.repository.as_url(), "https://github.com/owner/repo");
    assert_eq!(request.recipe, DocumentRecipe::OriginalBody);
}

#[test]
fn graph_policy_overrides_reach_the_engine_request() {
    let args = ClusterBuildArgs {
        threshold: 0.7,
        cross_kind_threshold: 0.95,
        fanout: 8,
        max_cluster_size: 20,
        min_cluster_size: 2,
        ..test_args()
    };

    let request = args
        .prepare(
            EmbeddingServiceConfig::default(),
            DocumentRecipe::OriginalBody,
        )
        .expect("prepare request");

    assert_eq!(request.options.threshold, 0.7);
    assert_eq!(request.options.cross_kind_threshold, 0.95);
    assert_eq!(request.options.fanout, 8);
    assert_eq!(request.options.max_cluster_size, 20);
    assert_eq!(request.options.min_cluster_size, 2);
}

#[test]
fn nonlocal_http_endpoint_is_rejected_before_archive_execution() {
    let args = ClusterBuildArgs {
        endpoint: Some("http://vectors.example/v1".to_owned()),
        ..test_args()
    };

    let result = args.prepare(
        EmbeddingServiceConfig::default(),
        DocumentRecipe::OriginalBody,
    );

    assert!(matches!(result, Err(ConfigError::InvalidEmbeddings)));
}

/// Constructs one explicit parsed build scenario with the normal graph defaults.
fn test_args() -> ClusterBuildArgs {
    ClusterBuildArgs {
        repository: "owner/repo".parse().expect("repository selector"),
        endpoint: None,
        model: None,
        threshold: 0.80,
        cross_kind_threshold: 0.93,
        fanout: 16,
        max_cluster_size: 40,
        min_cluster_size: 1,
    }
}
