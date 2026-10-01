#![forbid(unsafe_code)]

//! Interactive terminal browser for a local Forgesync archive.
//!
//! Browsing, search, coverage, failures, and cluster inspection read the archive locally. Sync,
//! refresh, and retry contact GitHub through the supplied clients; cluster decisions write only
//! local state. [`run`] restores the terminal, stops background tasks, and closes the archive
//! before returning. It installs no tracing subscriber and chooses no exit code.
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

mod app;
mod event_loop;
mod query;
mod view;

use std::collections::HashMap;
use std::io::{self, IsTerminal};
use std::sync::Arc;

use forgesync_core::identity::GitHubHost;
use forgesync_github::transport::GitHubClient;
use forgesync_store::archive::Archive;
use thiserror::Error;
use tokio::runtime::Handle;

use crate::event_loop::EventLoop;
use crate::query::tasks::QueryTasks;

/// Runs the browser until the user quits, then closes the archive.
///
/// An empty client map still permits local browsing. Returns [`TuiError::NotTerminal`] when
/// standard input or output is redirected.
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
        let mut event_loop = EventLoop::new(
            Arc::clone(&archive),
            Arc::clone(&github_clients),
            &runtime,
            &mut tasks,
        );
        event_loop.run(terminal)
    });

    tasks.stop().await;
    if let Ok(archive) = Arc::try_unwrap(archive) {
        archive.close().await;
    }
    terminal_result.map_err(TuiError::Terminal)
}

#[derive(Debug, Error)]
pub enum TuiError {
    #[error("the tui command requires an interactive terminal")]
    NotTerminal,
    #[error("the tui command needs an active Tokio runtime")]
    RuntimeUnavailable,
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
