//! Archive command handling.

use super::*;

pub(super) async fn archive_command(
    path: &std::path::Path,
    json: bool,
    command: ArchiveCommand,
) -> ExitCode {
    match command {
        ArchiveCommand::Init => match Archive::create(path).await {
            Ok(archive) => {
                let info = archive.info().clone();
                archive.close().await;
                render_success(json, "archive init", &info, archive_summary)
            }
            Err(error) => render_store_error(json, "archive init", error),
        },
        ArchiveCommand::Migrate => match Archive::migrate(path).await {
            Ok(migration) => match Archive::open_read_only(path).await {
                Ok(archive) => {
                    let data = MigrationOutput {
                        migration,
                        archive: archive.info().clone(),
                    };
                    archive.close().await;
                    render_success(json, "archive migrate", &data, migration_summary)
                }
                Err(error) => render_store_error(json, "archive migrate", error),
            },
            Err(error) => render_store_error(json, "archive migrate", error),
        },
        ArchiveCommand::Status => match Archive::open_read_only(path).await {
            Ok(archive) => {
                let result = archive_status(&archive).await;
                archive.close().await;
                match result {
                    Ok(status) => {
                        let output = ArchiveStatusOutput::from(&status);
                        render_success(json, "archive status", &output, archive_status_summary)
                    }
                    Err(error) => render_engine_error(json, "archive status", error),
                }
            }
            Err(error) => render_store_error(json, "archive status", error),
        },
        ArchiveCommand::Doctor => match Archive::open_read_only(path).await {
            Ok(archive) => {
                let report = match archive.doctor().await {
                    Ok(report) => report,
                    Err(error) => {
                        archive.close().await;
                        return render_store_error(json, "archive doctor", error);
                    }
                };
                archive.close().await;
                let exit_status = if report.healthy {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                };
                render_result(json, "archive doctor", &report, doctor_summary, exit_status)
            }
            Err(error) => render_store_error(json, "archive doctor", error),
        },
    }
}
