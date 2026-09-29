//! Thread command handling.

use super::*;

pub(super) async fn thread_command(
    path: &std::path::Path,
    json: OutputMode,
    command: ThreadCommand,
) -> ExitCode {
    match command {
        ThreadCommand::List {
            repositories,
            kind,
            state,
            sort,
            limit,
            offset,
        } => match Archive::open_read_only(path).await {
            Ok(archive) => {
                let request = ThreadListRequest {
                    filters: thread_filters(repositories, kind, state, sort, limit, offset),
                };
                let result = list_threads(&archive, &request).await;
                archive.close().await;
                match result {
                    Ok(page) => render_thread_page(json, "thread list", &page),
                    Err(error) => render_engine_error(json, "thread list", error),
                }
            }
            Err(error) => render_store_error(json, "thread list", error),
        },
        ThreadCommand::Show { reference } => match Archive::open_read_only(path).await {
            Ok(archive) => {
                let result = show_thread(&archive, &reference).await;
                archive.close().await;
                match result {
                    Ok(detail) => render_thread_detail(json, &detail),
                    Err(error) => render_engine_error(json, "thread show", error),
                }
            }
            Err(error) => render_store_error(json, "thread show", error),
        },
    }
}
