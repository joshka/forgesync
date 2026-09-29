//! Executable entry point for the Forgesync command-line application.
//!
//! Argument parsing, configuration, diagnostics, and exit-code selection live in the library's
//! process interface so this binary only delegates to it.

use std::process::ExitCode;

/// Delegates process argument handling and exit status selection to the CLI library.
fn main() -> ExitCode {
    forgesync_cli::run_from(std::env::args_os())
}
