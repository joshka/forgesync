#![forbid(unsafe_code)]

//! Process interface for the Forgesync application.

pub mod args;
pub mod config;
pub mod credentials;
pub mod output;

mod commands;
mod reports;

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
#[cfg(feature = "tui")]
use std::io::IsTerminal;
use std::io::Write;
use std::process::ExitCode;

use args::{
    ArchiveCommand, CliArgs, ClusterCommand, Command, LogFormat, RefreshAnalysisArg, RunCommand,
    RunFamilyArg, SearchModeArg, SyncIncludeArg, SyncThreadStateArg, ThreadCommand, ThreadKindArg,
    ThreadSortArg, ThreadStateArg,
};
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
use commands::dispatch;
use config::ForgesyncConfig;
use forgesync_core::content::{ReviewState, SourceState, ThreadKind, ThreadKind as DiscussionKind};
use forgesync_core::coverage::CoverageState;
use forgesync_core::document::DocumentRecipe;
use forgesync_core::identity::{GitHubHost, RunId};
use forgesync_core::outcome::OperationOutcome;
use forgesync_core::timestamp::UtcTimestamp;
use forgesync_engine::clustering::{
    ClusterBuildReport, ClusterBuildRequest, ClusterListRequest, ClusterOptions, build_clusters,
    dismiss_cluster, exclude_cluster_member, include_cluster_member, list_clusters,
    restore_cluster, set_canonical_cluster_member, show_cluster,
};
use forgesync_engine::embedding_client::EmbeddingClient;
use forgesync_engine::embeddings::EmbeddingReport;
use forgesync_engine::error::EngineError;
use forgesync_engine::inspect::{
    ThreadFilters, ThreadListRequest, ThreadSort, ThreadStateFilter, archive_status, list_threads,
    show_thread,
};
use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::refresh::{
    EmbeddingServiceIdentity, RefreshAnalysisStage, RefreshDocumentFailure, RefreshReport,
    RefreshRequest, RefreshStageFailure, RefreshStageKind, RefreshStageStatus, RefreshSyncOptions,
    embed_repositories, refresh,
};
use forgesync_engine::runs::{RetryReport, list_runs, plan_run_retry, run_retry, show_run};
use forgesync_engine::search::{SearchMode, SearchRequest, SearchResultPage, retrieve_threads};
use forgesync_engine::sync::{
    SyncProgress, SyncReport, SyncRequest, SyncThreadScope, sync_repositories,
};
use forgesync_github::transport::{GitHubClient, GitHubClientConfig};
use forgesync_store::archive::{Archive, ArchiveInfo};
use forgesync_store::clusters::{ClusterDetail, ClusterPage};
use forgesync_store::error::StoreError;
use forgesync_store::health::DoctorReport;
use forgesync_store::migration::MigrationReport;
use forgesync_store::reads::{ThreadDetail, ThreadPage, ThreadTimelineEvent};
use forgesync_store::runs::{RunDetail, RunRecord, RunStatus, SyncJobStatus};
use serde::Serialize;
use tracing_subscriber::filter::LevelFilter;

use crate::output::{
    ArchiveStatusOutput, JsonEnvelope, SearchPageOutput, ThreadDetailOutput, ThreadPageOutput,
};

/// Parses arguments, runs the selected command, and writes its process output.
pub fn run_from<I, T>(arguments: I) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let args = match CliArgs::try_parse_from(arguments) {
        Ok(args) => args,
        Err(error) => {
            let code = error.exit_code();
            let _ = error.print();
            return ExitCode::from(u8::try_from(code).unwrap_or(2));
        }
    };
    initialize_tracing(args.verbose, args.log_format);
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "Forgesync command started"
    );

    let config = match config_for_command(&args) {
        Ok(config) => config,
        Err(error) => {
            return render_error_with_status(
                args.json,
                "configuration",
                error.code(),
                &error.to_string(),
                ExitCode::from(2),
            );
        }
    };

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            return render_error(
                args.json,
                "startup",
                "runtime_unavailable",
                &format!("could not start async runtime: {error}"),
            );
        }
    };
    runtime.block_on(dispatch(args, config))
}

fn initialize_tracing(verbose: u8, format: LogFormat) {
    let max_level = match verbose {
        0 => LevelFilter::WARN,
        1 => LevelFilter::INFO,
        2 => LevelFilter::DEBUG,
        _ => LevelFilter::TRACE,
    };

    let result = match format {
        LogFormat::Text => tracing_subscriber::fmt()
            .compact()
            .with_writer(std::io::stderr)
            .with_max_level(max_level)
            .try_init(),
        LogFormat::Json => tracing_subscriber::fmt()
            .json()
            .flatten_event(true)
            .with_writer(std::io::stderr)
            .with_max_level(max_level)
            .try_init(),
    };
    if let Err(error) = result {
        eprintln!("could not initialize diagnostic logging: {error}");
    }
}

fn config_for_command(args: &CliArgs) -> Result<ForgesyncConfig, config::ConfigError> {
    #[cfg(feature = "tui")]
    if matches!(&args.command, Command::Tui) {
        return Ok(ForgesyncConfig::default());
    }

    ForgesyncConfig::load(args.config.as_deref())
}

fn render_success<T>(
    json: bool,
    command: &str,
    data: &T,
    human: impl FnOnce(&T) -> String,
) -> ExitCode
where
    T: Serialize,
{
    render_result(json, command, data, human, ExitCode::SUCCESS)
}

fn render_result<T>(
    json: bool,
    command: &str,
    data: &T,
    human: impl FnOnce(&T) -> String,
    exit_status: ExitCode,
) -> ExitCode
where
    T: Serialize,
{
    let mut stdout = std::io::stdout().lock();
    let result = if json {
        let envelope = JsonEnvelope::success(command, data);
        serde_json::to_writer(&mut stdout, &envelope)
            .and_then(|()| writeln!(stdout).map_err(serde_json::Error::io))
    } else {
        writeln!(stdout, "{}", human(data)).map_err(serde_json::Error::io)
    };
    match result {
        Ok(()) => exit_status,
        Err(error) if error.io_error_kind() == Some(std::io::ErrorKind::BrokenPipe) => exit_status,
        Err(_) => ExitCode::FAILURE,
    }
}

fn render_store_error(json: bool, command: &str, error: StoreError) -> ExitCode {
    render_error(json, command, error.code(), &error.to_string())
}

fn render_engine_error(json: bool, command: &str, error: EngineError) -> ExitCode {
    let code = error.code();
    let message = error.to_string();
    let status = if code == "operation_cancelled" {
        ExitCode::from(130)
    } else {
        ExitCode::FAILURE
    };
    render_error_with_status(json, command, code, &message, status)
}

fn render_error(json: bool, command: &str, code: &str, message: &str) -> ExitCode {
    render_error_with_status(json, command, code, message, ExitCode::FAILURE)
}

fn render_error_with_status(
    json: bool,
    command: &str,
    code: &str,
    message: &str,
    exit_status: ExitCode,
) -> ExitCode {
    if json {
        let envelope = JsonEnvelope::<serde_json::Value>::failure(command, code, message);
        if serde_json::to_writer(std::io::stdout().lock(), &envelope).is_ok() {
            let _ = writeln!(std::io::stdout().lock());
        }
    } else {
        let _ = writeln!(std::io::stderr().lock(), "forgesync: {message}");
    }
    exit_status
}

fn usage_error(message: &str) -> ExitCode {
    let mut command = CliArgs::command();
    let error = command.error(ErrorKind::MissingRequiredArgument, message.to_owned());
    let code = error.exit_code();
    let _ = error.print();
    ExitCode::from(u8::try_from(code).unwrap_or(2))
}
