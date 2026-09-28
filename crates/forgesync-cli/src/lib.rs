#![forbid(unsafe_code)]

//! Process interface for the Forgesync application.

pub mod args;
pub mod credentials;
pub mod output;

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Write;
use std::process::ExitCode;

use args::{
    ArchiveCommand, CliArgs, Command, SearchModeArg, SyncThreadStateArg, ThreadCommand,
    ThreadKindArg, ThreadSortArg, ThreadStateArg,
};
use clap::{CommandFactory, Parser, error::ErrorKind};
use forgesync_core::{
    CoverageState, GitHubHost, OperationOutcome, ReviewState, SourceState, ThreadKind,
    ThreadKind as DiscussionKind, UtcTimestamp,
};
use forgesync_engine::{
    EngineError, RepositorySelector, SearchMode, SearchRequest, SyncProgress, SyncReport,
    SyncRequest, SyncThreadScope, ThreadDetail, ThreadFilters, ThreadListRequest, ThreadPage,
    ThreadSort, ThreadStateFilter, archive_status, list_threads, search_threads, show_thread,
    sync_repositories,
};
use forgesync_github::{GitHubClient, GitHubClientConfig};
use forgesync_store::{
    Archive, ArchiveInfo, DoctorReport, MigrationReport, StoreError, ThreadTimelineEvent,
};
use serde::Serialize;

use crate::output::{ArchiveStatusOutput, JsonEnvelope, ThreadDetailOutput, ThreadPageOutput};

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
    runtime.block_on(dispatch(args))
}

async fn dispatch(args: CliArgs) -> ExitCode {
    let Some(path) = args.archive else {
        return usage_error("--archive PATH is required for local archive commands");
    };

    match args.command {
        Command::Archive { command } => match command {
            ArchiveCommand::Init => match Archive::create(&path).await {
                Ok(archive) => {
                    let info = archive.info().clone();
                    archive.close().await;
                    render_success(args.json, "archive init", &info, archive_summary)
                }
                Err(error) => render_store_error(args.json, "archive init", error),
            },
            ArchiveCommand::Migrate => match Archive::migrate(&path).await {
                Ok(migration) => match Archive::open_read_only(&path).await {
                    Ok(archive) => {
                        let data = MigrationOutput {
                            migration,
                            archive: archive.info().clone(),
                        };
                        archive.close().await;
                        render_success(args.json, "archive migrate", &data, migration_summary)
                    }
                    Err(error) => render_store_error(args.json, "archive migrate", error),
                },
                Err(error) => render_store_error(args.json, "archive migrate", error),
            },
            ArchiveCommand::Status => match Archive::open_read_only(&path).await {
                Ok(archive) => {
                    let result = archive_status(&archive).await;
                    archive.close().await;
                    match result {
                        Ok(status) => {
                            let output = ArchiveStatusOutput::from(&status);
                            render_success(
                                args.json,
                                "archive status",
                                &output,
                                archive_status_summary,
                            )
                        }
                        Err(error) => render_engine_error(args.json, "archive status", error),
                    }
                }
                Err(error) => render_store_error(args.json, "archive status", error),
            },
            ArchiveCommand::Doctor => match Archive::open_read_only(&path).await {
                Ok(archive) => {
                    let report = archive.doctor().await;
                    archive.close().await;
                    let exit_status = if report.healthy {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    };
                    render_result(
                        args.json,
                        "archive doctor",
                        &report,
                        doctor_summary,
                        exit_status,
                    )
                }
                Err(error) => render_store_error(args.json, "archive doctor", error),
            },
        },
        Command::Search {
            query,
            repositories,
            kind,
            state,
            mode,
            sort,
            limit,
            offset,
        } => match Archive::open_read_only(&path).await {
            Ok(archive) => {
                let request = SearchRequest {
                    query,
                    mode: match mode {
                        SearchModeArg::Keyword => SearchMode::Keyword,
                        SearchModeArg::AdvancedFts => SearchMode::AdvancedFts,
                    },
                    filters: thread_filters(repositories, kind, state, sort, limit, offset),
                };
                let result = search_threads(&archive, &request).await;
                archive.close().await;
                match result {
                    Ok(page) => render_thread_page(args.json, "search", &page),
                    Err(error) => render_engine_error(args.json, "search", error),
                }
            }
            Err(error) => render_store_error(args.json, "search", error),
        },
        Command::Sync {
            repositories,
            all,
            state,
        } => {
            let cancellation = tokio_util::sync::CancellationToken::new();
            let interrupt_cancellation = cancellation.clone();
            let interrupt_task = tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    interrupt_cancellation.cancel();
                }
            });
            let result = sync_command(
                &path,
                repositories,
                all,
                state,
                args.json,
                args.verbose,
                &cancellation,
            )
            .await;
            interrupt_task.abort();
            result
        }
        Command::Thread { command } => match command {
            ThreadCommand::List {
                repositories,
                kind,
                state,
                sort,
                limit,
                offset,
            } => match Archive::open_read_only(&path).await {
                Ok(archive) => {
                    let request = ThreadListRequest {
                        filters: thread_filters(repositories, kind, state, sort, limit, offset),
                    };
                    let result = list_threads(&archive, &request).await;
                    archive.close().await;
                    match result {
                        Ok(page) => render_thread_page(args.json, "thread list", &page),
                        Err(error) => render_engine_error(args.json, "thread list", error),
                    }
                }
                Err(error) => render_store_error(args.json, "thread list", error),
            },
            ThreadCommand::Show { reference } => match Archive::open_read_only(&path).await {
                Ok(archive) => {
                    let result = show_thread(&archive, &reference).await;
                    archive.close().await;
                    match result {
                        Ok(detail) => render_thread_detail(args.json, &detail),
                        Err(error) => render_engine_error(args.json, "thread show", error),
                    }
                }
                Err(error) => render_store_error(args.json, "thread show", error),
            },
        },
    }
}

fn thread_filters(
    repositories: Vec<forgesync_engine::RepositorySelector>,
    kind: Option<ThreadKindArg>,
    state: ThreadStateArg,
    sort: Option<ThreadSortArg>,
    limit: u32,
    offset: u64,
) -> ThreadFilters {
    ThreadFilters {
        repositories,
        kind: kind.map(|kind| match kind {
            ThreadKindArg::Issue => ThreadKind::Issue,
            ThreadKindArg::Pr => ThreadKind::PullRequest,
        }),
        state: match state {
            ThreadStateArg::All => ThreadStateFilter::All,
            ThreadStateArg::Open => ThreadStateFilter::Open,
            ThreadStateArg::Closed => ThreadStateFilter::Closed,
        },
        sort: sort.map(|sort| match sort {
            ThreadSortArg::Relevance => ThreadSort::Relevance,
            ThreadSortArg::Updated => ThreadSort::Updated,
            ThreadSortArg::Created => ThreadSort::Created,
        }),
        limit,
        offset,
    }
}

async fn sync_command(
    archive_path: &std::path::Path,
    repositories: Vec<RepositorySelector>,
    all: bool,
    state: Option<SyncThreadStateArg>,
    json: bool,
    verbose: u8,
    cancellation: &tokio_util::sync::CancellationToken,
) -> ExitCode {
    let archive = match Archive::open_read_write(archive_path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(json, "sync", error),
    };
    let selectors = if all {
        match archive.list_repositories().await {
            Ok(registered) => registered
                .iter()
                .map(RepositorySelector::from_repository)
                .collect::<Vec<_>>(),
            Err(error) => {
                archive.close().await;
                return render_store_error(json, "sync", error);
            }
        }
    } else {
        repositories.clone()
    };
    let mut hosts = selectors
        .iter()
        .map(|selector| selector.host().clone())
        .collect::<Vec<_>>();
    hosts.sort();
    hosts.dedup();

    let mut clients = HashMap::with_capacity(hosts.len());
    for host in hosts {
        let token = match crate::credentials::resolve_github_token(
            &crate::credentials::GitHubCredentialSettings::default(),
            &host,
            cancellation,
        )
        .await
        {
            Ok(token) => Some(token),
            Err(
                crate::credentials::CredentialError::NoCredential
                | crate::credentials::CredentialError::CommandUnavailable
                | crate::credentials::CredentialError::CommandFailed
                | crate::credentials::CredentialError::TimedOut,
            ) => {
                if verbose > 0 {
                    eprintln!("forgesync: no usable GitHub token for {host}; trying anonymously");
                }
                None
            }
            Err(crate::credentials::CredentialError::Cancelled) => {
                archive.close().await;
                return render_result(
                    json,
                    "sync",
                    &SyncFailure {
                        code: "operation_cancelled",
                        message: "sync was cancelled before acquisition began".to_owned(),
                    },
                    |failure| failure.message.clone(),
                    ExitCode::from(130),
                );
            }
            Err(error) => {
                archive.close().await;
                return render_error(
                    json,
                    "sync",
                    "github_credential_invalid",
                    &error.to_string(),
                );
            }
        };
        let base_url = github_api_base_url(&host);
        let config = match url::Url::parse(&base_url) {
            Ok(url) => GitHubClientConfig::new(url),
            Err(_) => {
                archive.close().await;
                return render_error(
                    json,
                    "sync",
                    "github_api_url_invalid",
                    "could not build GitHub API URL",
                );
            }
        };
        match GitHubClient::new(config, token) {
            Ok(client) => {
                clients.insert(host, client);
            }
            Err(error) => {
                archive.close().await;
                return render_error(
                    json,
                    "sync",
                    "github_client_initialization_failed",
                    &error.to_string(),
                );
            }
        }
    }

    let (progress_sender, mut progress_receiver) = tokio::sync::mpsc::channel::<SyncProgress>(4);
    let progress_task = if verbose > 0 && !json {
        Some(tokio::spawn(async move {
            while let Some(progress) = progress_receiver.recv().await {
                let repository = progress.repository.as_deref().unwrap_or("sync");
                eprintln!(
                    "forgesync: {}: {:?}, {}/{} jobs, {} threads",
                    repository,
                    progress.status,
                    progress.completed_jobs,
                    progress.total_jobs,
                    progress.threads_seen
                );
            }
        }))
    } else {
        drop(progress_receiver);
        None
    };
    let request = SyncRequest {
        repositories,
        all,
        scope: match state {
            None => SyncThreadScope::Default,
            Some(SyncThreadStateArg::Open) => SyncThreadScope::Open,
            Some(SyncThreadStateArg::Closed) => SyncThreadScope::Closed,
            Some(SyncThreadStateArg::All) => SyncThreadScope::All,
        },
    };
    let result = sync_repositories(
        &archive,
        &clients,
        &request,
        cancellation,
        Some(progress_sender),
    )
    .await;
    if let Some(progress_task) = progress_task {
        let _ = progress_task.await;
    }
    archive.close().await;
    match result {
        Ok(report) => {
            let exit_status = outcome_exit_code(&report.outcome);
            render_result(json, "sync", &report, sync_summary, exit_status)
        }
        Err(error) => render_engine_error(json, "sync", error),
    }
}

fn github_api_base_url(host: &GitHubHost) -> String {
    if host.as_str() == "github.com" {
        "https://api.github.com/".to_owned()
    } else {
        format!("{}/api/v3/", host.https_origin())
    }
}

fn outcome_exit_code(outcome: &OperationOutcome) -> ExitCode {
    match outcome {
        OperationOutcome::Complete => ExitCode::SUCCESS,
        OperationOutcome::Partial { .. } | OperationOutcome::Deferred { .. } => ExitCode::from(3),
        OperationOutcome::Interrupted { .. } => ExitCode::from(130),
        OperationOutcome::Failed { .. } => ExitCode::FAILURE,
    }
}

fn sync_summary(report: &SyncReport) -> String {
    let state = match report.outcome {
        OperationOutcome::Complete => "complete",
        OperationOutcome::Partial { .. } => "partial",
        OperationOutcome::Deferred { .. } => "deferred",
        OperationOutcome::Interrupted { .. } => "interrupted",
        OperationOutcome::Failed { .. } => "failed",
    };
    format!(
        "Sync {state}: {} repositories, {}/{} jobs complete, {} failed, {} deferred, {} pages, {} threads",
        report.repositories_selected,
        report.completed_jobs,
        report.total_jobs,
        report.failed_jobs,
        report.deferred_jobs,
        report.pages_completed,
        report.threads_seen
    )
}

#[derive(Serialize)]
struct SyncFailure {
    code: &'static str,
    message: String,
}

fn render_thread_page(json: bool, command: &str, page: &ThreadPage) -> ExitCode {
    let output = ThreadPageOutput::from(page);
    render_success(json, command, &output, thread_page_summary)
}

fn render_thread_detail(json: bool, detail: &ThreadDetail) -> ExitCode {
    let output = ThreadDetailOutput::from(detail);
    render_success(json, "thread show", &output, thread_detail_summary)
}

fn archive_status_summary(status: &ArchiveStatusOutput<'_>) -> String {
    let mut lines = vec![
        archive_summary(status.archive),
        format!(
            "Repositories: {}\nThreads: {} ({} issues, {} pull requests)",
            status.repositories, status.threads, status.issues, status.pull_requests
        ),
        "Coverage:".to_owned(),
    ];
    lines.extend(status.coverage.iter().map(|coverage| {
        format!(
            "  {}: {} complete, {} incomplete, {} missing of {}",
            family_name(coverage.family),
            coverage.complete,
            coverage.incomplete,
            coverage.missing,
            coverage.applicable_threads
        )
    }));
    lines.join("\n")
}

fn thread_page_summary(page: &ThreadPageOutput<'_>) -> String {
    let mut lines = vec!["REPOSITORY\tNUMBER\tKIND\tSTATE\tTITLE".to_owned()];
    for item in &page.items {
        let thread = item.thread;
        lines.push(format!(
            "{}\t{}\t{}\t{}\t{}",
            item.repository.full_name,
            thread.id.number().get(),
            discussion_kind_name(thread.kind),
            source_state_name(&thread.state),
            thread.title
        ));
    }
    if page.items.is_empty() {
        lines.push("No discussions matched.".to_owned());
    }
    lines.push("Coverage:".to_owned());
    lines.extend(page.coverage.iter().map(|coverage| {
        format!(
            "  {}: {} complete, {} incomplete, {} missing of {}",
            family_name(coverage.family),
            coverage.complete,
            coverage.incomplete,
            coverage.missing,
            coverage.applicable_threads
        )
    }));
    if let Some(next_offset) = page.next_offset {
        lines.push(format!("Next offset: {next_offset}"));
    }
    lines.join("\n")
}

fn thread_detail_summary(detail: &ThreadDetailOutput<'_>) -> String {
    let thread = detail.summary.thread;
    let mut lines = vec![format!(
        "{}/{}#{} — {}\nKind: {}\nState: {}\nUpdated: {}",
        detail.summary.repository.owner,
        detail.summary.repository.name,
        thread.id.number().get(),
        thread.title,
        discussion_kind_name(thread.kind),
        source_state_name(&thread.state),
        format_timestamp(thread.updated_at)
    )];
    if let Some(url) = &thread.html_url {
        lines.push(format!("URL: {url}"));
    }
    if let Some(body) = &thread.body {
        lines.push(String::new());
        lines.push(body.clone());
    }
    lines.push(String::new());
    lines.push("Coverage:".to_owned());
    lines.extend(detail.summary.coverage.iter().map(|coverage| {
        format!(
            "  {}: {}",
            family_name(coverage.family()),
            coverage_state_name(coverage.state())
        )
    }));
    for item in detail.pull_request_metadata {
        let metadata = &item.payload;
        lines.push(String::new());
        lines.push(format!(
            "Pull request: {}:{} -> {}:{} (draft: {})",
            repository_identity(metadata.head.repository.as_ref()),
            metadata.head.name,
            repository_identity(metadata.base.repository.as_ref()),
            metadata.base.name,
            metadata.draft
        ));
    }

    if !detail.timeline.is_empty() {
        lines.push(String::new());
        lines.push("Current timeline:".to_owned());
        for entry in detail.timeline {
            let time = entry
                .occurred_at
                .map(format_timestamp)
                .unwrap_or_else(|| "time unavailable".to_owned());
            let summary = match &entry.event {
                ThreadTimelineEvent::ThreadCreated { thread, title } => {
                    format!(
                        "{}#{} opened: {title}",
                        thread.repository().provider_id(),
                        thread.number().get()
                    )
                }
                ThreadTimelineEvent::ThreadClosed { thread } => {
                    format!(
                        "{}#{} closed",
                        thread.repository().provider_id(),
                        thread.number().get()
                    )
                }
                ThreadTimelineEvent::Comment { comment } => format!(
                    "comment by {}: {}",
                    comment.author.as_deref().unwrap_or("unknown author"),
                    comment.body
                ),
                ThreadTimelineEvent::Review { review } => format!(
                    "review {}: {}{}",
                    review.id.provider_id(),
                    review_state_name(&review.state),
                    review
                        .body
                        .as_deref()
                        .map_or(String::new(), |body| format!(" — {body}"))
                ),
                ThreadTimelineEvent::ReviewThread {
                    path,
                    is_resolved,
                    is_outdated,
                    ..
                } => format!(
                    "review thread {}: {}{}",
                    path.as_deref().unwrap_or("unknown path"),
                    if *is_resolved {
                        "resolved"
                    } else {
                        "unresolved"
                    },
                    if *is_outdated { ", outdated" } else { "" }
                ),
                ThreadTimelineEvent::ReviewThreadComment {
                    path,
                    is_resolved,
                    is_outdated,
                    comment,
                    ..
                } => format!(
                    "review comment on {} ({}{}), by {}: {}",
                    path.as_deref().unwrap_or("unknown path"),
                    if *is_resolved {
                        "resolved"
                    } else {
                        "unresolved"
                    },
                    if *is_outdated { ", outdated" } else { "" },
                    comment.author.as_deref().unwrap_or("unknown author"),
                    comment.body
                ),
            };
            lines.push(format!("  {time}: {summary}"));
        }
    }
    lines.join("\n")
}

fn discussion_kind_name(kind: DiscussionKind) -> &'static str {
    match kind {
        DiscussionKind::Issue => "issue",
        DiscussionKind::PullRequest => "pull request",
    }
}

fn source_state_name(state: &SourceState) -> &str {
    match state {
        SourceState::Open => "open",
        SourceState::Closed => "closed",
        SourceState::Other(value) => value,
    }
}

fn review_state_name(state: &ReviewState) -> &str {
    match state {
        ReviewState::Approved => "approved",
        ReviewState::ChangesRequested => "changes requested",
        ReviewState::Commented => "commented",
        ReviewState::Dismissed => "dismissed",
        ReviewState::Pending => "pending",
        ReviewState::Other(value) => value,
    }
}

fn repository_identity(repository: Option<&forgesync_core::RepositoryId>) -> String {
    repository.map_or_else(
        || "unknown repository".to_owned(),
        |repository| {
            format!(
                "{}/{}",
                repository.host(),
                repository.provider_id().as_str()
            )
        },
    )
}

fn family_name(family: forgesync_core::EvidenceFamily) -> &'static str {
    match family {
        forgesync_core::EvidenceFamily::Threads => "threads",
        forgesync_core::EvidenceFamily::Comments => "comments",
        forgesync_core::EvidenceFamily::PullRequestMetadata => "pull_request_metadata",
        forgesync_core::EvidenceFamily::Reviews => "reviews",
        forgesync_core::EvidenceFamily::ReviewThreads => "review_threads",
    }
}

fn coverage_state_name(state: &CoverageState) -> &'static str {
    match state {
        CoverageState::Missing => "missing",
        CoverageState::Incomplete { .. } => "incomplete",
        CoverageState::Complete { .. } => "complete",
        CoverageState::Unavailable { .. } => "unavailable",
        CoverageState::Failed { .. } => "failed",
        CoverageState::Deferred { .. } => "deferred",
    }
}

fn format_timestamp(timestamp: UtcTimestamp) -> String {
    timestamp
        .format_rfc3339()
        .unwrap_or_else(|_| "invalid timestamp".to_owned())
}

#[derive(Serialize)]
struct MigrationOutput {
    migration: MigrationReport,
    archive: ArchiveInfo,
}

fn archive_summary(info: &ArchiveInfo) -> String {
    format!(
        "Archive: {}\nID: {}\nFormat: {}\nSchema: {}\nCreated: {}\nSQLite: {}",
        info.path,
        info.archive_id,
        info.format_id,
        info.schema_version,
        info.created_at
            .format_rfc3339()
            .unwrap_or_else(|_| "invalid timestamp".to_owned()),
        info.sqlite_version
    )
}

fn migration_summary(output: &MigrationOutput) -> String {
    let applied = output.migration.applied_migrations.len();
    format!(
        "Archive schema is at version {} ({} migration{} applied)",
        output.migration.schema_version,
        applied,
        if applied == 1 { "" } else { "s" }
    )
}

fn doctor_summary(report: &DoctorReport) -> String {
    let headline = if report.healthy {
        "Archive health: healthy"
    } else {
        "Archive health: unhealthy"
    };
    let checks = report
        .checks
        .iter()
        .map(|check| {
            let state = if check.healthy { "ok" } else { "failed" };
            format!("[{state}] {}: {}", check.name, check.detail)
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("{headline}\n{checks}")
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
    render_error(json, command, error.code(), &error.to_string())
}

fn render_error(json: bool, command: &str, code: &str, message: &str) -> ExitCode {
    if json {
        let envelope = JsonEnvelope::<serde_json::Value>::failure(command, code, message);
        if serde_json::to_writer(std::io::stdout().lock(), &envelope).is_ok() {
            let _ = writeln!(std::io::stdout().lock());
        }
    } else {
        let _ = writeln!(std::io::stderr().lock(), "forgesync: {message}");
    }
    ExitCode::FAILURE
}

fn usage_error(message: &str) -> ExitCode {
    let mut command = CliArgs::command();
    let error = command.error(ErrorKind::MissingRequiredArgument, message.to_owned());
    let code = error.exit_code();
    let _ = error.print();
    ExitCode::from(u8::try_from(code).unwrap_or(2))
}
