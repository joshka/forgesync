use assert_cmd::Command;

fn forgesync() -> Command {
    Command::new(env!("CARGO_BIN_EXE_forgesync"))
}

#[test]
fn version_flag_prints_package_version() {
    let output = forgesync().arg("--version").output().expect("run binary");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("version is UTF-8");
    assert!(stdout.starts_with("forgesync "));
}

#[test]
fn help_lists_global_options_and_no_deferred_commands() {
    let output = forgesync().arg("--help").output().expect("run binary");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help is UTF-8");
    for option in ["--archive", "--config", "--json", "--color", "--log-format"] {
        assert!(stdout.contains(option), "help must contain {option}");
    }
    for deferred in ["portable", "cloud", "summarize", "code index", "serve"] {
        assert!(
            !stdout.contains(deferred),
            "help must not advertise {deferred}"
        );
    }
}

#[test]
fn json_does_not_change_clap_usage_errors() {
    let output = forgesync().arg("--json").output().expect("run binary");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic is UTF-8");
    assert!(stderr.contains("a command is required"));
}
