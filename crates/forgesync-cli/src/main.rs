use std::process::ExitCode;

fn main() -> ExitCode {
    match forgesync_cli::run_from(std::env::args_os()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let code = error.exit_code();
            error.print().expect("write Clap diagnostic");
            ExitCode::from(u8::try_from(code).unwrap_or(2))
        }
    }
}
