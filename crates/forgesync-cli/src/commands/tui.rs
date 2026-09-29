//! Interactive terminal startup.

use std::io::IsTerminal;
use std::process::ExitCode;

use forgesync_engine::reference::RepositorySelector;
use forgesync_store::archive::Archive;

use super::github::{github_clients_for_selectors, render_github_client_setup_error};
use crate::{OutputMode, render_error, render_store_error, usage_error};

pub async fn tui_command(path: &std::path::Path, json: OutputMode, verbose: u8) -> ExitCode {
    if json.is_json() {
        return usage_error("--json is not supported by the interactive tui command");
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return render_error(
            OutputMode::Text,
            "tui",
            "tui_requires_terminal",
            "the tui command requires an interactive terminal",
        );
    }
    let archive = match Archive::open_read_write(path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(OutputMode::Text, "tui", error),
    };
    let registered = match archive.list_repositories().await {
        Ok(registered) => registered,
        Err(error) => {
            archive.close().await;
            return render_store_error(OutputMode::Text, "tui", error);
        }
    };
    let selectors = registered
        .iter()
        .map(RepositorySelector::from_repository)
        .collect::<Vec<_>>();
    let cancellation = tokio_util::sync::CancellationToken::new();
    let clients = match github_clients_for_selectors(&selectors, verbose, &cancellation).await {
        Ok(clients) => clients,
        Err(error) => {
            archive.close().await;
            return render_github_client_setup_error(OutputMode::Text, "tui", error);
        }
    };
    let result = forgesync_tui::run(archive, clients).await;
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => render_error(OutputMode::Text, "tui", error.code(), &error.to_string()),
    }
}
