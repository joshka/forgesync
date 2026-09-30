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
//!
//! # Invoke the process boundary
//!
//! Use [`run_from`] only when a caller needs the complete CLI behavior, including stream output,
//! configuration resolution, tracing setup, and exit status. Pass the program name as the first
//! argument. Engine APIs are the better entry point when another Rust component needs a structured
//! result without process effects.
//!
//! ```no_run
//! let status = forgesync_cli::run_from(["forgesync", "--help"]);
//! assert_eq!(status, std::process::ExitCode::SUCCESS);
//! ```
//!
//! # Follow one command
//!
//! Parsed command types and their execution live together in private `command` modules. Execution
//! resolves configuration, opens the required archive mode, creates explicit provider services when
//! needed, runs an engine operation, and closes the archive before rendering its result. [`output`]
//! owns JSON envelopes; private `reports` owns human summaries. Errors are translated at this
//! boundary so libraries retain typed causes and the process presents stable codes.
//!
//! [`config`] describes layered local settings and validation. [`credentials`] resolves secrets at
//! the application boundary; credentials never belong in reports, tracing fields, or stored
//! provider payloads. The default `tui` feature includes terminal browsing. Building without
//! default features retains the noninteractive archive, inspection, acquisition, search, and
//! cluster commands.
//!
//! The Rust command wiring is an evolving application implementation. Documented CLI envelopes and
//! persisted archive behavior are maintained deliberately, without development-era argument
//! aliases.

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
use command::CliArgs;
use command::values::LogFormat;
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
        Err(error) => return render_argument_error(error),
    };
    if let Some(status) = args.validate_process() {
        return status;
    }
    initialize_tracing(args.verbose, args.log_format);
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "Forgesync command started"
    );

    let config = match config_for_command(&args) {
        Ok(config) => config,
        Err(error) => return render_configuration_error(OutputMode::from(args.json), error),
    };

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => return render_runtime_error(OutputMode::from(args.json), error),
    };
    runtime.block_on(args.dispatch(config))
}

/// Presents rejected configuration before any command archive or provider operation begins.
///
/// Configuration errors use their stable typed code and status 2. The selected output mode controls
/// JSON on stdout versus the human diagnostic on stderr through the shared renderer.
fn render_configuration_error(output: OutputMode, error: config::ConfigError) -> ExitCode {
    let message = error.to_string();
    render_error_with_status(
        output,
        "configuration",
        error.code(),
        &message,
        ExitCode::from(2),
    )
}

/// Presents failure to create the process-owned async runtime as a startup error.
///
/// No command has been dispatched. The stable code is independent of the underlying I/O wording,
/// which remains useful in the human or JSON diagnostic message.
fn render_runtime_error(output: OutputMode, error: std::io::Error) -> ExitCode {
    let message = format!("could not start async runtime: {error}");
    render_error(output, "startup", "runtime_unavailable", &message)
}

/// Writes Clap's help or usage diagnostic to its selected stream and preserves its exit status.
///
/// Help/version requests may return success through this path. Printing failure is ignored, as
/// argument interpretation already selected the status; an unrepresentable code falls back to 2.
fn render_argument_error(error: clap::Error) -> ExitCode {
    let code = error.exit_code();
    let _ = error.print();
    ExitCode::from(u8::try_from(code).unwrap_or(2))
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
        LogFormat::Text => initialize_text_tracing(max_level),
        LogFormat::Json => initialize_json_tracing(max_level),
    };
    if let Err(error) = result {
        eprintln!("could not initialize diagnostic logging: {error}");
    }
}

/// Installs compact human diagnostics on stderr at the resolved process verbosity.
///
/// Returns subscriber-installation failure to the process coordinator, which reports it and
/// continues command execution. This never changes stdout command output formatting.
fn initialize_text_tracing(
    max_level: LevelFilter,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .compact()
        .with_writer(std::io::stderr)
        .with_max_level(max_level)
        .try_init()
}

/// Installs JSON diagnostics with flattened event fields on stderr at process verbosity.
///
/// This format governs tracing only; command JSON envelopes have their own output policy.
/// Installation failure remains a startup diagnostic rather than changing command exit status.
fn initialize_json_tracing(
    max_level: LevelFilter,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_writer(std::io::stderr)
        .with_max_level(max_level)
        .try_init()
}

/// Selects the configuration source needed by the chosen command.
fn config_for_command(args: &CliArgs) -> Result<ForgesyncConfig, config::ConfigError> {
    ForgesyncConfig::load(args.config.as_deref())
}

/// Process presentation policy resolved once from the global JSON option.
///
/// Commands receive this value instead of a behavior-selecting boolean. Diagnostic and success
/// renderers own stream selection; the policy does not select acquisition or storage behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputMode {
    /// Human summaries on stdout and human errors on stderr.
    Text,
    /// Versioned command envelopes on stdout, independent of tracing's stderr encoding.
    Json,
}

impl From<bool> for OutputMode {
    /// Converts the parsed JSON flag once at the process boundary into the typed rendering policy.
    /// Downstream command and report methods receive this policy instead of a behavioral boolean.
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
    render_argument_error(error)
}
