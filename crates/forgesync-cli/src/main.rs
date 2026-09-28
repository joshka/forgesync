use std::process::ExitCode;

fn main() -> ExitCode {
    forgesync_cli::run_from(std::env::args_os())
}
