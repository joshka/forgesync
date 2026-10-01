//! Launch the terminal browser.
//!
//! Startup prepares GitHub clients for every registered repository's host, which can discover
//! credentials but acquires no evidence. The archive and clients are then handed to the TUI, which
//! owns cancellation, terminal restoration, and archive shutdown.

use std::io::IsTerminal;
use std::path::Path;

use forgesync_engine::reference::RepositorySelector;
use forgesync_store::archive::Archive;
use forgesync_tui::TuiError;
use tokio_util::sync::CancellationToken;

use crate::command::github::github_clients_for_selectors;
use crate::error::{CliError, Exit};

/// Rejects noninteractive streams before configuration or archive access.
pub fn check_terminal() -> Result<(), CliError> {
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        Ok(())
    } else {
        Err(TuiError::NotTerminal.into())
    }
}

pub async fn run_tui(path: &Path, verbose: u8) -> Result<Exit, CliError> {
    let archive = Archive::open_read_write(path).await?;
    let clients = async {
        let selectors = archive
            .list_repositories()
            .await?
            .iter()
            .map(RepositorySelector::from_repository)
            .collect::<Vec<_>>();
        github_clients_for_selectors(&selectors, verbose, &CancellationToken::new()).await
    }
    .await;
    match clients {
        Ok(clients) => {
            forgesync_tui::run(archive, clients).await?;
            Ok(Exit::Success)
        }
        Err(error) => {
            archive.close().await;
            Err(error)
        }
    }
}
