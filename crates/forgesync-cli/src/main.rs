//! # Process entry point for the Forgesync CLI
//!
//! The binary delegates parsing and execution to the library's `run_from` entry point, then
//! returns its exit code to the shell. Command behavior, configuration resolution, and output
//! formatting live in library modules so they can be read without following process bootstrap
//! code.
//!
//! Keep this file intentionally small. A new command belongs under `command`, and any new terminal
//! or JSON representation belongs under `reports` or `output`.

use std::process::ExitCode;

/// Delegates process argument handling and exit status selection to the CLI library.
fn main() -> ExitCode {
    forgesync_cli::run_from(std::env::args_os())
}
