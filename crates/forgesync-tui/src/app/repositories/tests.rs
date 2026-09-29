//! # Repository scope survives asynchronous refresh
//!
//! These scenarios protect the picker boundary: the highlighted list row and applied repository
//! have different jobs. Refreshing or clearing the list must not change the repository used by
//! writer actions. A stale reply must not finish the current read or overwrite its safe error.
//!
//! The fixture supplies one explicit repository identity and selected picker. Each test then
//! performs a single read transition and inspects its result; no provider or terminal is needed.
//! General keyboard selection and rendering remain covered by the app and view suites.

use forgesync_core::content::Repository;
use forgesync_core::identity::{GitHubHost, ProviderId, RepositoryId};
use forgesync_core::provider_data::ProviderData;
use rstest::{fixture, rstest};

use super::RepositoryPicker;
use crate::app::App;

/// A picker with one applied repository, independent of the synthetic all-repositories row.
#[fixture]
fn selected_picker() -> RepositoryPicker {
    let repository = Repository {
        id: RepositoryId::new(
            GitHubHost::parse("github.com").expect("host"),
            ProviderId::new("41").expect("provider ID"),
        ),
        owner: "owner".to_owned(),
        name: "selected".to_owned(),
        full_name: "owner/selected".to_owned(),
        default_branch: None,
        updated_at: None,
        provider_data: ProviderData::new(),
    };
    RepositoryPicker {
        cursor: 1,
        items: vec![repository.clone()],
        applied: Some(repository),
        ..RepositoryPicker::default()
    }
}

#[rstest]
fn inserted_repository_does_not_retarget_the_applied_writer_scope(
    mut selected_picker: RepositoryPicker,
) {
    let selected = selected_picker.items[0].clone();
    let inserted = Repository {
        id: RepositoryId::new(
            GitHubHost::parse("github.com").expect("host"),
            ProviderId::new("42").expect("provider ID"),
        ),
        name: "inserted".to_owned(),
        full_name: "owner/inserted".to_owned(),
        ..selected.clone()
    };
    let generation = selected_picker.begin();
    let error = selected_picker.apply(generation, Ok(vec![inserted, selected]));
    assert_eq!(error, None);
    let app = App {
        repository_picker: selected_picker,
        ..App::default()
    };
    let scope = app.repository_scope();
    assert_eq!(scope.len(), 1);
    assert_eq!(scope[0].as_url(), "https://github.com/owner/selected");
}

#[rstest]
fn empty_refresh_does_not_broaden_the_applied_writer_scope(mut selected_picker: RepositoryPicker) {
    let generation = selected_picker.begin();
    let error = selected_picker.apply(generation, Ok(Vec::new()));
    assert_eq!(error, None);
    assert_eq!(selected_picker.cursor, 0);
    let app = App {
        repository_picker: selected_picker,
        ..App::default()
    };
    let scope = app.repository_scope();
    assert_eq!(scope.len(), 1);
    assert_eq!(scope[0].as_url(), "https://github.com/owner/selected");
}

#[rstest]
fn stale_failure_does_not_finish_a_newer_repository_read(mut selected_picker: RepositoryPicker) {
    let stale = selected_picker.begin();
    let current = selected_picker.begin();
    let error = selected_picker.apply(stale, Err("old read failed".to_owned()));
    assert_eq!(error, None);
    assert_eq!(selected_picker.generation, current);
    assert!(selected_picker.loading);
    assert_eq!(selected_picker.error, None);
    assert_eq!(selected_picker.items[0].full_name, "owner/selected");
}

#[rstest]
fn failed_refresh_retains_rows_and_the_applied_scope(mut selected_picker: RepositoryPicker) {
    let generation = selected_picker.begin();
    let error = selected_picker.apply(generation, Err("archive read failed".to_owned()));
    assert_eq!(error.as_deref(), Some("archive read failed"));
    assert_eq!(
        selected_picker.error.as_deref(),
        Some("archive read failed")
    );
    assert!(!selected_picker.loading);
    assert_eq!(selected_picker.items[0].full_name, "owner/selected");
    let app = App {
        repository_picker: selected_picker,
        ..App::default()
    };
    assert_eq!(
        app.repository_scope()[0].as_url(),
        "https://github.com/owner/selected"
    );
}

#[rstest]
fn renamed_repository_updates_the_applied_scope_by_provider_identity(
    mut selected_picker: RepositoryPicker,
) {
    let mut renamed = selected_picker.items[0].clone();
    renamed.name = "renamed".to_owned();
    renamed.full_name = "owner/renamed".to_owned();
    let generation = selected_picker.begin();
    let error = selected_picker.apply(generation, Ok(vec![renamed]));
    assert_eq!(error, None);
    let app = App {
        repository_picker: selected_picker,
        ..App::default()
    };
    let scope = app.repository_scope();
    assert_eq!(scope.len(), 1);
    assert_eq!(scope[0].as_url(), "https://github.com/owner/renamed");
}
