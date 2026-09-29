#![forbid(unsafe_code)]

//! # Interactive local archive browser
//!
//! The TUI opens a terminal session around an already selected archive and lets a user browse
//! threads, coverage, failures, and duplicate clusters. It depends on engine and core concepts,
//! not CLI command modules. The CLI launches it and handles process concerns; this crate owns
//! interaction and drawing.
//!
//! `app` holds navigation state and processes input, `query` runs archive operations without
//! blocking the event loop, and `view` draws the current screen. The `run` entry point coordinates
//! terminal setup, events, and cleanup. `TuiError` carries failures to the caller for user-facing
//! reporting.
//!
//! # Launch and own the terminal session
//!
//! [`run`] consumes an opened archive and restores the terminal before returning. It also stops its
//! background read and writer tasks and closes the handle. Call it from a Tokio runtime with
//! standard input and output connected to a terminal. The returned [`TuiError`] belongs to the
//! caller's process error policy; this crate does not install a tracing subscriber or select an
//! exit code.
//!
//! ```no_run
//! use std::collections::HashMap;
//!
//! use forgesync_store::archive::Archive;
//!
//! # async fn browse() -> Result<(), Box<dyn std::error::Error>> {
//! let archive = Archive::open_read_write("archive.sqlite3").await?;
//! // Empty provider clients permit local browsing; acquisition actions need configured clients.
//! forgesync_tui::run(archive, HashMap::new()).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Follow an interaction
//!
//! The private `app` module maps keys to typed actions and owns screen state. `query` schedules
//! work, forwards progress, and tags results with a generation. `app` discards results from
//! superseded requests before updating state. `view` renders that state and never starts archive
//! operations. Local reads remain responsive while a writer runs; quitting requests cancellation
//! and waits for its cleanup instead of abandoning a durable operation.
//!
//! The default browser is local, while explicit sync and refresh actions may contact configured
//! providers. Local cluster decisions update the archive and never write to GitHub. The CLI's `tui`
//! feature controls whether the launcher is included; this crate itself owns the terminal behavior.

mod app;
mod query;
mod view;

use std::collections::HashMap;
use std::io::{self, IsTerminal};
use std::sync::Arc;
use std::time::Duration;

use app::App;
use app::messages::QueryMessage;
use crossterm::event::{self, Event, KeyEventKind};
use forgesync_core::identity::GitHubHost;
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use query::{QueryTasks, start_query};
use thiserror::Error;
use tokio::runtime::Handle;
use tokio::sync::mpsc;

/// Runs the interactive archive browser and closes its archive handle on exit.
///
/// Supply clients keyed by their validated GitHub host. An empty map still permits local browsing;
/// provider-backed actions require a matching client. This function returns
/// [`TuiError::NotTerminal`] when standard input or output is redirected.
pub async fn run(
    archive: Archive,
    github_clients: HashMap<GitHubHost, GitHubClient>,
) -> Result<(), TuiError> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        archive.close().await;
        return Err(TuiError::NotTerminal);
    }
    let runtime = match Handle::try_current() {
        Ok(runtime) => runtime,
        Err(_) => {
            archive.close().await;
            return Err(TuiError::RuntimeUnavailable);
        }
    };
    let archive = Arc::new(archive);
    let github_clients = Arc::new(github_clients);
    let mut tasks = QueryTasks::default();
    let terminal_result = ratatui::run(|terminal| {
        run_event_loop(
            terminal,
            Arc::clone(&archive),
            Arc::clone(&github_clients),
            &runtime,
            &mut tasks,
        )
    });

    tasks.stop().await;
    if let Ok(archive) = Arc::try_unwrap(archive) {
        archive.close().await;
    }
    terminal_result.map_err(TuiError::Terminal)
}

/// Keeps keyboard input responsive while completed background messages update the view.
fn run_event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    archive: Arc<Archive>,
    github_clients: Arc<HashMap<GitHubHost, GitHubClient>>,
    runtime: &Handle,
    tasks: &mut QueryTasks,
) -> io::Result<()> {
    let (sender, mut receiver) = mpsc::channel(16);
    let mut app = App::default();
    for action in app.initial_actions() {
        start_query(
            action,
            &mut app,
            &archive,
            &github_clients,
            runtime,
            &sender,
            tasks,
        );
    }

    while !app.quit {
        while let Ok(message) = receiver.try_recv() {
            let operation_finished = matches!(message, QueryMessage::OperationFinished { .. });
            app.apply(message);
            if operation_finished {
                for action in app.refresh_after_operation() {
                    start_query(
                        action,
                        &mut app,
                        &archive,
                        &github_clients,
                        runtime,
                        &sender,
                        tasks,
                    );
                }
            }
        }

        terminal.draw(|frame| view::draw(frame, &mut app))?;
        if event::poll(Duration::from_millis(40))?
            && let Event::Key(key) = event::read()?
            && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            for action in app.handle_key(key) {
                start_query(
                    action,
                    &mut app,
                    &archive,
                    &github_clients,
                    runtime,
                    &sender,
                    tasks,
                );
            }
        }
    }

    Ok(())
}

/// An error starting or running the terminal browser.
#[derive(Debug, Error)]
pub enum TuiError {
    /// The browser needs an interactive stdin and stdout.
    #[error("the tui command requires an interactive terminal")]
    NotTerminal,
    /// No Tokio runtime is active to run background archive queries.
    #[error("the tui command needs an active Tokio runtime")]
    RuntimeUnavailable,
    /// Terminal setup, input, drawing, or restoration failed.
    #[error("terminal I/O failed: {0}")]
    Terminal(#[from] io::Error),
}

impl TuiError {
    /// Returns the stable CLI error classification.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotTerminal => "tui_requires_terminal",
            Self::RuntimeUnavailable => "tui_runtime_unavailable",
            Self::Terminal(_) => "tui_terminal_io",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TuiError;

    #[test]
    fn tui_errors_have_stable_cli_codes() {
        assert_eq!(TuiError::NotTerminal.code(), "tui_requires_terminal");
        assert_eq!(
            TuiError::RuntimeUnavailable.code(),
            "tui_runtime_unavailable"
        );
    }
}
