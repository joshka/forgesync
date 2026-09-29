//! # Coverage projection retention and stale reads
//!
//! These scenarios protect refreshing presentation state, not SQLite diagnostics themselves.
//! A prior successful projection stays available while another read runs or fails. Only the current
//! generation may replace it or finish loading; stale success is as unsafe as stale failure.
//!
//! The fixture is an explicit illustrative status with seven discussions and no pending work.
//! It opens no archive and does not claim that its display metadata describes an installed SQLite
//! or real migration history. Store tests separately validate acquisition of those diagnostics.

use forgesync_core::timestamp::UtcTimestamp;
use forgesync_store::archive::ArchiveInfo;
use forgesync_store::diagnostics::{
    ArchiveDiagnostics, ArchiveLeaseStatus, SchemaDiagnostics, WorkDiagnostics,
};
use forgesync_store::reads::ArchiveStatus;
use rstest::{fixture, rstest};

use super::CoveragePanel;

/// Fixed display data whose discussion count makes retained versus replaced projection visible.
#[fixture]
fn status() -> ArchiveStatus {
    let timestamp = UtcTimestamp::parse("2026-09-29T00:00:00Z").expect("timestamp");
    ArchiveStatus {
        archive: ArchiveInfo {
            path: "fixture.sqlite".to_owned(),
            archive_id: "00000000-0000-0000-0000-000000000001".to_owned(),
            format_id: "forgesync".to_owned(),
            created_at: timestamp,
            schema_version: 1,
            sqlite_version: "3.50.4".to_owned(),
        },
        repositories: 2,
        threads: 7,
        issues: 5,
        pull_requests: 2,
        coverage: Vec::new(),
        diagnostics: ArchiveDiagnostics {
            schema: SchemaDiagnostics {
                current_version: 1,
                supported_version: 1,
                pending_migrations: Vec::new(),
                history_valid: true,
            },
            lease: ArchiveLeaseStatus {
                owner_id: None,
                fencing_token: 0,
                expires_at: timestamp,
                held: false,
            },
            work: WorkDiagnostics {
                failed_jobs: 0,
                deferred_jobs: 0,
                in_progress_runs: 0,
                failures_by_family: Vec::new(),
                unassigned_failures: 0,
                unresolved_failures: 0,
            },
        },
    }
}

#[rstest]
fn pending_refresh_retains_the_last_successful_projection(status: ArchiveStatus) {
    let mut panel = CoveragePanel {
        data: Some(status),
        error: Some("previous failure".to_owned()),
        ..Default::default()
    };
    assert_eq!(panel.begin(), 1);
    assert_eq!(panel.data.as_ref().expect("retained projection").threads, 7);
    assert!(panel.loading);
    assert_eq!(panel.error, None);
}

#[rstest]
fn failed_refresh_retains_projection_and_exposes_the_current_error(status: ArchiveStatus) {
    let mut panel = CoveragePanel {
        data: Some(status),
        ..Default::default()
    };
    assert_eq!(panel.begin(), 1);
    assert_eq!(
        panel.apply(1, Err("read failed".to_owned())).as_deref(),
        Some("read failed")
    );
    assert_eq!(panel.data.as_ref().expect("retained projection").threads, 7);
    assert_eq!(panel.error.as_deref(), Some("read failed"));
    assert!(!panel.loading);
}

#[rstest]
fn stale_success_cannot_finish_loading_or_replace_the_current_projection(status: ArchiveStatus) {
    let mut newer = status.clone();
    newer.threads = 8;
    newer.issues = 6;
    let mut panel = CoveragePanel {
        data: Some(status),
        ..Default::default()
    };
    assert_eq!(panel.begin(), 1);
    assert_eq!(panel.begin(), 2);
    assert_eq!(panel.apply(1, Ok(Box::new(newer))), None);
    assert_eq!(panel.data.as_ref().expect("retained projection").threads, 7);
    assert_eq!(panel.generation, 2);
    assert!(panel.loading);
}

#[rstest]
fn current_success_replaces_projection_and_finishes_loading(mut status: ArchiveStatus) {
    let mut panel = CoveragePanel {
        data: Some(status.clone()),
        ..Default::default()
    };
    assert_eq!(panel.begin(), 1);
    status.threads = 8;
    status.issues = 6;
    assert_eq!(panel.apply(1, Ok(Box::new(status))), None);
    assert_eq!(panel.data.as_ref().expect("current projection").threads, 8);
    assert_eq!(panel.error, None);
    assert!(!panel.loading);
}
