#![forbid(unsafe_code)]

//! # Forgesync command-line application
//!
//! This crate turns user arguments and local configuration into explicit engine workflows. The CLI
//! owns process concerns: parsing, credential discovery, tracing setup, exit codes, and human or
//! JSON presentation. Library crates receive opened archives and typed requests; they do not read
//! process configuration or choose how to print results. Credentials never belong in reports,
//! tracing fields, or stored provider payloads.
//!
//! Use [`run_from`] only when a caller needs the complete CLI behavior, including stream output,
//! configuration resolution, tracing setup, and exit status. Engine APIs are the better entry point
//! when another Rust component needs a structured result without process effects.
//!
//! ```no_run
//! let status = forgesync_cli::run_from(["forgesync", "--help"]);
//! assert_eq!(status, std::process::ExitCode::SUCCESS);
//! ```

mod command;
mod config;
mod credentials;
mod error;
mod output;
mod reports;

use std::ffi::OsString;
use std::io::IsTerminal;
use std::process::ExitCode;

use clap::Parser;
use command::CliArgs;
use command::values::{ColorChoice, LogFormat};
use config::ForgesyncConfig;
use error::CliError;
use output::{Output, OutputMode, render_argument_error};
use tracing_indicatif::IndicatifLayer;
use tracing_indicatif::filter::IndicatifFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Parses arguments, runs the selected command, and writes its process output.
///
/// The first argument is the program name, as with [`std::env::args_os`]. This function creates
/// its own Tokio runtime and tracing subscriber for the invocation.
pub fn run_from<I, T>(arguments: I) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let args = match CliArgs::try_parse_from(arguments) {
        Ok(args) => args,
        Err(error) => return render_argument_error(&error),
    };
    let mode = OutputMode::from(args.json);
    // Terminal failures take precedence over unrelated configuration errors.
    #[cfg(feature = "tui")]
    if let Err(error) = args.check_terminal() {
        return Output {
            mode,
            command: "tui",
        }
        .error(&error);
    }
    initialize_tracing(&args);
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "Forgesync command started"
    );

    let configuration = Output {
        mode,
        command: "configuration",
    };
    let config = match ForgesyncConfig::load(args.config.as_deref()) {
        Ok(config) => config,
        Err(error) => return configuration.error(&error.into()),
    };
    let path = match config.archive.resolve(args.archive.as_deref()) {
        Ok(path) => path,
        Err(error) => return configuration.error(&error.into()),
    };
    tracing::info!(archive = %path.display(), "Selected archive");

    let runtime = match command_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            let startup = Output {
                mode,
                command: "startup",
            };
            return startup.error(&CliError::Runtime(error));
        }
    };
    runtime.block_on(args.dispatch(&path, config))
}

/// Builds the process runtime for timers, provider I/O, credential subprocesses, and Ctrl-C.
fn command_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod runtime_tests;

/// Installs diagnostics on stderr at the requested verbosity and encoding.
///
/// Installation failure is reported but does not change the command's exit status.
fn initialize_tracing(args: &CliArgs) {
    let max_level = match args.verbose {
        0 => LevelFilter::WARN,
        1 => LevelFilter::INFO,
        2 => LevelFilter::DEBUG,
        _ => LevelFilter::TRACE,
    };
    let ansi = tracing_ansi(args.color, std::env::var_os("NO_COLOR"));

    let result = match args.log_format {
        LogFormat::Text if !args.json && std::io::stderr().is_terminal() => {
            initialize_progress_tracing(max_level, ansi)
        }
        LogFormat::Text => tracing_subscriber::fmt()
            .compact()
            .with_ansi(ansi)
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

/// Selects ANSI styling for human diagnostics.
///
/// `auto` keeps tracing-subscriber's default: styled unless `NO_COLOR` is set to a nonempty value.
fn tracing_ansi(color: ColorChoice, no_color: Option<OsString>) -> bool {
    match color {
        ColorChoice::Auto => no_color.is_none_or(|value| value.is_empty()),
        ColorChoice::Always => true,
        ColorChoice::Never => false,
    }
}

/// Coordinates human logs with opt-in acquisition spans on interactive stderr.
///
/// Only spans that opt in create progress bars. Filtering diagnostics independently keeps progress
/// visible at default verbosity without enabling informational logs.
fn initialize_progress_tracing(
    max_level: LevelFilter,
    ansi: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let progress = IndicatifLayer::new();
    let diagnostics = tracing_subscriber::fmt::layer()
        .compact()
        .with_ansi(ansi)
        .with_writer(progress.get_stderr_writer())
        .with_filter(max_level);
    tracing_subscriber::registry()
        .with(diagnostics)
        .with(progress.with_filter(IndicatifFilter::new(false)))
        .try_init()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use crate::command::values::ColorChoice;
    use crate::tracing_ansi;

    #[rstest::rstest]
    #[case::auto(ColorChoice::Auto, None, true)]
    #[case::auto_no_color(ColorChoice::Auto, Some("1"), false)]
    #[case::auto_empty_no_color(ColorChoice::Auto, Some(""), true)]
    #[case::always_overrides_no_color(ColorChoice::Always, Some("1"), true)]
    #[case::never(ColorChoice::Never, None, false)]
    fn color_choice_selects_diagnostic_styling(
        #[case] color: ColorChoice,
        #[case] no_color: Option<&str>,
        #[case] expected: bool,
    ) {
        assert_eq!(tracing_ansi(color, no_color.map(OsString::from)), expected);
    }
}
