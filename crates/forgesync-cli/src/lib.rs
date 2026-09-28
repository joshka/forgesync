#![forbid(unsafe_code)]

//! Process interface for the Forgesync application.

pub mod args;
pub mod output;

use std::ffi::OsString;

use clap::{CommandFactory, Parser, error::ErrorKind};

use crate::args::CliArgs;

/// Parses arguments and reports that the selected command surface is not yet available.
///
/// Feature commands are added with their implementation tasks. Until one is available, non-help
/// invocations fail as ordinary command-line usage errors.
pub fn run_from<I, T>(arguments: I) -> Result<(), clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let _args = CliArgs::try_parse_from(arguments)?;
    let mut command = CliArgs::command();
    Err(command.error(ErrorKind::MissingSubcommand, "a command is required"))
}
