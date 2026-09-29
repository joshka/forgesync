#![forbid(unsafe_code)]

//! Read-only terminal browser for local Forgesync archives.

mod app;
mod query;
mod view;

use std::io::{self, IsTerminal};
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};
use forgesync_store::Archive;
use thiserror::Error;
use tokio::runtime::Handle;
use tokio::sync::mpsc;

use app::App;
use query::{QueryTasks, start_query};

/// Runs the interactive archive browser and closes its read-only archive handle on exit.
pub async fn run(archive: Archive) -> Result<(), TuiError> {
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
    let mut tasks = QueryTasks::default();
    let terminal_result = ratatui::run(|terminal| {
        run_event_loop(terminal, Arc::clone(&archive), &runtime, &mut tasks)
    });

    tasks.stop().await;
    if let Ok(archive) = Arc::try_unwrap(archive) {
        archive.close().await;
    }
    terminal_result.map_err(TuiError::Terminal)
}

fn run_event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    archive: Arc<Archive>,
    runtime: &Handle,
    tasks: &mut QueryTasks,
) -> io::Result<()> {
    let (sender, mut receiver) = mpsc::channel(16);
    let mut app = App::default();
    for action in app.initial_actions() {
        start_query(action, &mut app, &archive, runtime, &sender, tasks);
    }

    while !app.quit {
        while let Ok(message) = receiver.try_recv() {
            app.apply(message);
        }

        terminal.draw(|frame| view::draw(frame, &mut app))?;
        if event::poll(Duration::from_millis(40))?
            && let Event::Key(key) = event::read()?
            && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            for action in app.handle_key(key) {
                start_query(action, &mut app, &archive, runtime, &sender, tasks);
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
