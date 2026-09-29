//! Cluster command handling.

use std::path::Path;
use std::process::ExitCode;

use forgesync_core::document::DocumentRecipe;
use tokio_util::sync::CancellationToken;

use crate::OutputMode;
use crate::args::ClusterCommand;
use crate::config::{EmbeddingServiceConfig, ForgesyncConfig};

mod build;
mod decisions;
mod read;

use build::build_cluster_command;
use decisions::{
    canonical_cluster_command, dismiss_cluster_command, exclude_cluster_command,
    include_cluster_command, restore_cluster_command,
};
use read::{list_cluster_command, show_cluster_command};

pub(super) async fn cluster_from_cli(
    command: ClusterCommand,
    path: &Path,
    json: OutputMode,
    verbose: u8,
    config: ForgesyncConfig,
) -> ExitCode {
    let cancellation = CancellationToken::new();
    let interrupt_cancellation = cancellation.clone();
    let interrupt_task = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            interrupt_cancellation.cancel();
        }
    });
    let result = cluster_command(
        path,
        command,
        config.embeddings,
        config.documents.recipe,
        json,
        verbose,
        &cancellation,
    )
    .await;
    interrupt_task.abort();
    result
}

pub(super) async fn cluster_command(
    archive_path: &Path,
    command: ClusterCommand,
    embedding_service: EmbeddingServiceConfig,
    recipe: DocumentRecipe,
    json: OutputMode,
    verbose: u8,
    cancellation: &CancellationToken,
) -> ExitCode {
    match command {
        ClusterCommand::Build(args) => {
            build_cluster_command(
                args,
                archive_path,
                embedding_service,
                recipe,
                json,
                verbose,
                cancellation,
            )
            .await
        }
        ClusterCommand::List(args) => list_cluster_command(args, archive_path, json).await,
        ClusterCommand::Show { id } => show_cluster_command(id, archive_path, json).await,
        ClusterCommand::Dismiss { id, reason } => {
            dismiss_cluster_command(id, reason, archive_path, json).await
        }
        ClusterCommand::Restore { id } => restore_cluster_command(id, archive_path, json).await,
        ClusterCommand::Exclude { id, member, reason } => {
            exclude_cluster_command(id, member, reason, archive_path, json).await
        }
        ClusterCommand::Include { id, member } => {
            include_cluster_command(id, member, archive_path, json).await
        }
        ClusterCommand::Canonical { id, member } => {
            canonical_cluster_command(id, member, archive_path, json).await
        }
    }
}
