#![forbid(unsafe_code)]

//! Process interface for the Forgesync application.

pub mod args;
pub mod output;

use std::ffi::OsString;
use std::io::Write;
use std::process::ExitCode;

use args::{ArchiveCommand, CliArgs, Command};
use clap::{CommandFactory, Parser, error::ErrorKind};
use forgesync_store::{Archive, ArchiveInfo, DoctorReport, MigrationReport, StoreError};
use serde::Serialize;

use crate::output::JsonEnvelope;

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
    let Command::Archive { command } = args.command;
    let Some(path) = args.archive else {
        return usage_error("--archive PATH is required for archive commands");
    };

    match command {
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
                let info = archive.info().clone();
                archive.close().await;
                render_success(args.json, "archive status", &info, archive_summary)
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
    }
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
    if result.is_err() {
        ExitCode::FAILURE
    } else {
        exit_status
    }
}

fn render_store_error(json: bool, command: &str, error: StoreError) -> ExitCode {
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
