//! # Scope process interruption to one command
//!
//! `CommandInterruption` owns a cancellation token and the asynchronous Ctrl-C listener that
//! cancels it. Commands borrow the token while executing engine workflows. Dropping the owner
//! aborts the listener, including when a command returns early or unwinds.
//!
//! The owner does not abort the workflow itself. Engine operations observe cancellation and retain
//! their structured interrupted reports and durable partial progress. This keeps archive cleanup
//! and writer-lease release with the workflow that acquired them.
//!
//! Signal handling belongs at the CLI process boundary. TUI key cancellation has its own operation
//! lifetime and does not use this listener. Construction requires an active Tokio runtime.

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Ctrl-C listener and the token borrowed by a single command's workflow.
pub struct CommandInterruption {
    cancellation: CancellationToken,
    listener: JoinHandle<()>,
}
impl CommandInterruption {
    /// Starts listening before command execution without changing workflow cleanup ownership.
    pub fn new() -> Self {
        let cancellation = CancellationToken::new();
        let requested = cancellation.clone();
        let listener = tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                requested.cancel();
            }
        });
        Self {
            cancellation,
            listener,
        }
    }

    /// Borrows the token whose cancellation is triggered by the process listener.
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }
}
impl Drop for CommandInterruption {
    /// Aborts the Ctrl-C listener at command-scope end, bounding signal handling to this
    /// invocation.
    fn drop(&mut self) {
        self.listener.abort();
    }
}
