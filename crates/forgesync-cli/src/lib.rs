#![forbid(unsafe_code)]

//! # Forgesync command-line application
//!
//! This crate turns user arguments and local configuration into explicit engine workflows. The CLI
//! owns process concerns: parsing, credential discovery, tracing setup, exit codes, and human or
//! JSON presentation. Library crates receive opened archives and typed requests; they do not read
//! process configuration or choose how to print results.
//!
//! `command` groups verbs by user task. `config` and `credentials` resolve local settings,
//! `output` defines stable JSON envelopes, and `reports` writes concise terminal summaries.
//! `run_from` is the reusable process entry point; `main` supplies the actual argument stream and
//! exit status.

mod command;
pub mod config;
pub mod credentials;
pub mod output;

mod reports;

use std::ffi::OsString;
use std::io::Write;
use std::process::ExitCode;

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
#[cfg(feature = "tui")]
use command::Command;
use command::{CliArgs, LogFormat};
use config::ForgesyncConfig;
use forgesync_engine::error::EngineError;
use forgesync_store::error::StoreError;
use serde::Serialize;
use tracing_subscriber::filter::LevelFilter;

use crate::output::JsonEnvelope;

/// Parses arguments, runs the selected command, and writes its process output.
///
/// The first argument is the program name, as with [`std::env::args_os`]. Parsing failures and
/// command failures are rendered to standard streams and represented by the returned exit code.
/// This function creates its own Tokio runtime and tracing subscriber for the invocation.
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
                OutputMode::from(args.json),
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
                OutputMode::from(args.json),
                "startup",
                "runtime_unavailable",
                &format!("could not start async runtime: {error}"),
            );
        }
    };
    runtime.block_on(args.dispatch(config))
}

/// Installs process-owned diagnostics at the requested verbosity and encoding.
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

/// Selects the configuration source needed by the chosen command.
fn config_for_command(args: &CliArgs) -> Result<ForgesyncConfig, config::ConfigError> {
    #[cfg(feature = "tui")]
    if matches!(&args.command, Command::Tui) {
        return Ok(ForgesyncConfig::default());
    }

    ForgesyncConfig::load(args.config.as_deref())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputMode {
    Text,
    Json,
}

impl From<bool> for OutputMode {
    fn from(json: bool) -> Self {
        if json { Self::Json } else { Self::Text }
    }
}

impl OutputMode {
    /// Selects structured output at rendering boundaries after the CLI flag has been resolved.
    const fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

/// Renders a successful operation while keeping JSON and human presentation in one place.
fn render_success<T>(
    json: OutputMode,
    command: &str,
    data: &T,
    human: impl FnOnce(&T) -> String,
) -> ExitCode
where
    T: Serialize,
{
    render_result(json, command, data, human, ExitCode::SUCCESS)
}

/// Writes the command result to stdout in the selected format. A closed output pipe is treated
/// as successful consumption rather than turning a completed operation into a process failure.
fn render_result<T>(
    json: OutputMode,
    command: &str,
    data: &T,
    human: impl FnOnce(&T) -> String,
    exit_status: ExitCode,
) -> ExitCode
where
    T: Serialize,
{
    let mut stdout = std::io::stdout().lock();
    let result = if json.is_json() {
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

/// Preserves the store error classification in process output.
fn render_store_error(json: OutputMode, command: &str, error: StoreError) -> ExitCode {
    render_error(json, command, error.code(), &error.to_string())
}

/// Preserves cancellation as exit status 130 while keeping the engine's stable error code in the
/// JSON envelope or human diagnostic.
fn render_engine_error(json: OutputMode, command: &str, error: EngineError) -> ExitCode {
    let code = error.code();
    let message = error.to_string();
    let status = if code == "operation_cancelled" {
        ExitCode::from(130)
    } else {
        ExitCode::FAILURE
    };
    render_error_with_status(json, command, code, &message, status)
}

/// Writes an application error in the selected output format.
fn render_error(json: OutputMode, command: &str, code: &str, message: &str) -> ExitCode {
    render_error_with_status(json, command, code, message, ExitCode::FAILURE)
}

/// Writes an application error and returns its selected process status.
fn render_error_with_status(
    json: OutputMode,
    command: &str,
    code: &str,
    message: &str,
    exit_status: ExitCode,
) -> ExitCode {
    if json.is_json() {
        let envelope = JsonEnvelope::<serde_json::Value>::failure(command, code, message);
        if serde_json::to_writer(std::io::stdout().lock(), &envelope).is_ok() {
            let _ = writeln!(std::io::stdout().lock());
        }
    } else {
        let _ = writeln!(std::io::stderr().lock(), "forgesync: {message}");
    }
    exit_status
}

/// Reports invalid command usage with the conventional usage exit code.
fn usage_error(message: &str) -> ExitCode {
    let mut command = CliArgs::command();
    let error = command.error(ErrorKind::MissingRequiredArgument, message.to_owned());
    let code = error.exit_code();
    let _ = error.print();
    ExitCode::from(u8::try_from(code).unwrap_or(2))
}
