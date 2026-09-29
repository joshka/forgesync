//! # Refresh preparation and failure-boundary contracts
//!
//! These cases inspect prepared engine requests before archive or provider acquisition. They keep
//! stage order, optional service capabilities, and selected sync families visible in direct
//! assertions. Invalid optional configuration must leave the selected stage in the request so the
//! engine can report partial progress rather than silently dropping that stage.
//!
//! Cluster-only preparation never needs an embedding client or credential lookup. Typed boundary
//! errors retain their engine source until the command's outer lifetime closes the archive.
//! End-to-end refresh behavior and durable partial reports are covered by engine workflow tests;
//! these tests establish the parsed-command translation that those engine tests do not exercise.

use std::error::Error;

use forgesync_engine::error::EngineError;
use forgesync_engine::refresh::RefreshAnalysisStage;
use forgesync_engine::sync::SyncThreadScope;

use crate::command::refresh::{RefreshArgs, RefreshFailure};
use crate::command::values::{RefreshAnalysisArg, SyncIncludeArg, SyncThreadStateArg};
use crate::config::ForgesyncConfig;

#[test]
fn cluster_only_preparation_uses_stored_vector_identity_without_a_client() {
    let args = RefreshArgs {
        repositories: vec!["owner/repo".parse().expect("repository")],
        no_sync: true,
        state: None,
        with: Vec::new(),
        analyze: vec![RefreshAnalysisArg::Clusters],
        force: false,
    };
    let mut config = ForgesyncConfig::default();
    config.embeddings.endpoint = "https://example.com/v1/".to_owned();
    config.embeddings.model = "  local-model  ".to_owned();

    let prepared = args.prepare(config);

    assert!(prepared.request.sync.is_none());
    assert_eq!(prepared.request.analysis, [RefreshAnalysisStage::Clusters]);
    assert!(prepared.embedding_client.is_none());
    let identity = prepared
        .request
        .embedding_identity
        .expect("stored vector identity");
    assert_eq!(identity.endpoint, "https://example.com/v1");
    assert_eq!(identity.model, "local-model");
}

#[test]
fn explicit_sync_state_and_families_reach_engine_scope() {
    let args = RefreshArgs {
        repositories: vec!["owner/repo".parse().expect("repository")],
        no_sync: false,
        state: Some(SyncThreadStateArg::All),
        with: vec![
            SyncIncludeArg::Comments,
            SyncIncludeArg::Reviews,
            SyncIncludeArg::ReviewThreads,
        ],
        analyze: Vec::new(),
        force: false,
    };

    let scope = args.sync_options().expect("selected sync scope");

    assert_eq!(scope.scope, SyncThreadScope::All);
    assert!(scope.include_comments);
    assert!(scope.include_reviews);
    assert!(scope.include_review_threads);
}

#[test]
fn unusable_optional_service_preserves_stage_order_and_force_policy() {
    let args = RefreshArgs {
        repositories: vec!["owner/repo".parse().expect("repository")],
        no_sync: true,
        state: None,
        with: Vec::new(),
        analyze: vec![RefreshAnalysisArg::Clusters, RefreshAnalysisArg::Embeddings],
        force: true,
    };
    let mut config = ForgesyncConfig::default();
    config.embeddings.endpoint = "not a service URL".to_owned();

    let prepared = args.prepare(config);

    assert_eq!(
        prepared.request.analysis,
        [
            RefreshAnalysisStage::Clusters,
            RefreshAnalysisStage::Embeddings
        ]
    );
    assert!(prepared.request.force_embeddings);
    assert!(prepared.embedding_client.is_none());
    assert!(prepared.request.embedding_identity.is_none());
}

#[test]
fn duplicate_stage_selection_fails_before_preparation() {
    let args = RefreshArgs {
        repositories: vec!["owner/repo".parse().expect("repository")],
        no_sync: true,
        state: None,
        with: Vec::new(),
        analyze: vec![RefreshAnalysisArg::Clusters, RefreshAnalysisArg::Clusters],
        force: false,
    };

    assert_eq!(
        args.validate(),
        Err("refresh analysis stages must be selected only once")
    );
}

#[test]
fn refresh_boundary_retains_the_typed_engine_cause() {
    let failure = RefreshFailure::Engine(EngineError::InvalidSyncScope);

    let source = failure.source().expect("engine source");

    assert!(source.downcast_ref::<EngineError>().is_some());
    assert_eq!(
        source.to_string(),
        EngineError::InvalidSyncScope.to_string()
    );
}

#[tokio::test]
async fn local_only_refresh_skips_provider_setup_even_after_cancellation() {
    let args = RefreshArgs {
        repositories: vec!["owner/repo".parse().expect("repository")],
        no_sync: true,
        state: None,
        with: Vec::new(),
        analyze: vec![RefreshAnalysisArg::Clusters],
        force: false,
    };
    let cancellation = tokio_util::sync::CancellationToken::new();
    cancellation.cancel();

    let clients = args
        .clients(0, &cancellation)
        .await
        .expect("local-only preparation");

    assert!(clients.is_empty());
}
