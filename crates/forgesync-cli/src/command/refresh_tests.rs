//! Refresh preparation contracts.
//!
//! Invalid optional configuration must leave the selected stage in the request so the engine can
//! report it, rather than silently dropping that stage. Cluster-only preparation needs no
//! embedding client or credential lookup.

use forgesync_engine::refresh::RefreshStageKind;
use forgesync_engine::sync::SyncThreadScope;

use crate::command::refresh::RefreshArgs;
use crate::command::sync::SyncScopeArgs;
use crate::command::values::{RefreshAnalysisArg, SyncIncludeArg, SyncThreadStateArg};
use crate::config::ForgesyncConfig;
use crate::error::CliError;

#[test]
fn cluster_only_preparation_uses_stored_vector_identity_without_a_client() {
    let args = RefreshArgs {
        repositories: vec!["owner/repo".parse().expect("repository")],
        no_sync: true,
        scope: SyncScopeArgs::default(),
        analyze: vec![RefreshAnalysisArg::Clusters],
        force: false,
    };
    let mut config = ForgesyncConfig::default();
    config.embeddings.endpoint = "https://example.com/v1/".to_owned();
    config.embeddings.model = "  local-model  ".to_owned();

    let prepared = args.prepare(config);

    assert!(prepared.request.sync.is_none());
    assert_eq!(prepared.request.analysis, [RefreshStageKind::Clusters]);
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
        scope: SyncScopeArgs {
            state: Some(SyncThreadStateArg::All),
            with: vec![
                SyncIncludeArg::Comments,
                SyncIncludeArg::Reviews,
                SyncIncludeArg::ReviewThreads,
            ],
        },
        analyze: Vec::new(),
        force: false,
    };

    let prepared = args.prepare(ForgesyncConfig::default());
    let scope = prepared.request.sync.expect("selected sync scope");

    assert_eq!(scope.scope, SyncThreadScope::All);
    assert!(scope.include_comments);
    assert!(scope.include_reviews);
    assert!(scope.include_review_threads);
}

#[test]
fn unusable_optional_service_preserves_selected_stages_and_force_policy() {
    let args = RefreshArgs {
        repositories: vec!["owner/repo".parse().expect("repository")],
        no_sync: true,
        scope: SyncScopeArgs::default(),
        analyze: vec![RefreshAnalysisArg::Clusters, RefreshAnalysisArg::Embeddings],
        force: true,
    };
    let mut config = ForgesyncConfig::default();
    config.embeddings.endpoint = "not a service URL".to_owned();

    let prepared = args.prepare(config);

    assert_eq!(
        prepared.request.analysis,
        [RefreshStageKind::Clusters, RefreshStageKind::Embeddings]
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
        scope: SyncScopeArgs::default(),
        analyze: vec![RefreshAnalysisArg::Clusters, RefreshAnalysisArg::Clusters],
        force: false,
    };

    assert!(matches!(
        args.validate(),
        Err(CliError::Usage(
            "refresh analysis stages must be selected only once"
        ))
    ));
}

#[tokio::test]
async fn local_only_refresh_skips_provider_setup_even_after_cancellation() {
    let args = RefreshArgs {
        repositories: vec!["owner/repo".parse().expect("repository")],
        no_sync: true,
        scope: SyncScopeArgs::default(),
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
