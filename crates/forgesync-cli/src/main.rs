//! Executable entry point for the Forgesync command-line application.
//!
//! Argument parsing, configuration, diagnostics, and exit-code selection live in the library's
//! process interface so this binary only delegates to it.

use std::process::ExitCode;

fn main() -> ExitCode {
    forgesync_cli::run_from(std::env::args_os())
}
