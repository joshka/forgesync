//! The `run_from` runtime supports Ctrl-C registration and subprocess I/O.

use std::future::Future;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use crate::command_runtime;

#[test]
fn command_runtime_registers_ctrl_c_without_receiving_a_signal() {
    let runtime = command_runtime().expect("build production CLI runtime");

    runtime.block_on(async {
        let signal = tokio::signal::ctrl_c();
        tokio::pin!(signal);
        let mut context = Context::from_waker(Waker::noop());
        assert!(matches!(signal.as_mut().poll(&mut context), Poll::Pending));
    });
}

#[test]
fn command_runtime_executes_subprocess_with_captured_output() {
    let runtime = command_runtime().expect("build production CLI runtime");
    let executable = std::env::current_exe().expect("locate test harness");

    let output = runtime.block_on(async {
        let mut command = tokio::process::Command::new(executable);
        command.arg("--list").kill_on_drop(true);
        tokio::time::timeout(Duration::from_secs(10), command.output())
            .await
            .expect("subprocess completes before timeout")
            .expect("execute test harness subprocess")
    });

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 test listing");
    assert!(
        stdout.contains("runtime_tests::command_runtime_executes_subprocess_with_captured_output")
    );
}
