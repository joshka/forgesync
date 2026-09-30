//! # Process entry point for the Forgesync CLI
//!
//! This binary passes operating-system arguments to the library's `run_from` entry point and
//! returns its selected exit code to the shell. Using OS strings preserves arguments until the
//! parser applies its own conversion rules; this entry point does not reinterpret them.
//!
//! The library owns argument parsing, configuration resolution, diagnostic setup, command dispatch,
//! and output rendering. Those process responsibilities remain at the CLI boundary rather than
//! being installed by the core, store, provider, or engine libraries. The synchronous library
//! adapter also owns the runtime needed to execute asynchronous commands.
//!
//! Keep this file small: a new operation belongs in the command tree, and a new human or JSON
//! representation belongs in its report or output owner. This entry point adds no implicit archive
//! creation, migration, or refresh before the selected command runs.

use std::process::ExitCode;

/// Delegates process argument handling and exit status selection to the CLI library.
fn main() -> ExitCode {
    forgesync_cli::run_from(std::env::args_os())
}
