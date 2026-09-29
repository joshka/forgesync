//! Interactive terminal startup.

use super::*;

pub(super) struct TuiCommandRequest<'a> {
    pub(super) path: &'a std::path::Path,
    pub(super) json: bool,
    pub(super) verbose: u8,
}

pub(super) async fn tui_command(request: TuiCommandRequest<'_>) -> ExitCode {
    if request.json {
        return usage_error("--json is not supported by the interactive tui command");
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return render_error(
            false,
            "tui",
            "tui_requires_terminal",
            "the tui command requires an interactive terminal",
        );
    }
    let archive = match Archive::open_read_write(request.path).await {
        Ok(archive) => archive,
        Err(error) => return render_store_error(false, "tui", error),
    };
    let registered = match archive.list_repositories().await {
        Ok(registered) => registered,
        Err(error) => {
            archive.close().await;
            return render_store_error(false, "tui", error);
        }
    };
    let selectors = registered
        .iter()
        .map(RepositorySelector::from_repository)
        .collect::<Vec<_>>();
    let cancellation = tokio_util::sync::CancellationToken::new();
    let clients =
        match github_clients_for_selectors(&selectors, request.verbose, &cancellation).await {
            Ok(clients) => clients,
            Err(error) => {
                archive.close().await;
                return render_github_client_setup_error(false, "tui", error);
            }
        };
    let result = forgesync_tui::run(archive, clients).await;
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => render_error(false, "tui", error.code(), &error.to_string()),
    }
}
