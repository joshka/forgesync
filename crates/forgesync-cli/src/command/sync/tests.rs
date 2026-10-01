//! Parsed acquisition scope reaches the engine request unchanged.

use forgesync_engine::reference::RepositorySelector;
use forgesync_engine::sync::SyncThreadScope;

use crate::command::sync::{SyncArgs, SyncScopeArgs};
use crate::command::values::{SyncIncludeArg, SyncThreadStateArg};

#[rstest::rstest]
#[case::default(None, SyncThreadScope::Default)]
#[case::open(Some(SyncThreadStateArg::Open), SyncThreadScope::Open)]
#[case::closed(Some(SyncThreadStateArg::Closed), SyncThreadScope::Closed)]
#[case::all(Some(SyncThreadStateArg::All), SyncThreadScope::All)]
fn parsed_state_becomes_domain_scope(
    #[case] state: Option<SyncThreadStateArg>,
    #[case] expected: SyncThreadScope,
) {
    let args = SyncArgs {
        repositories: Vec::new(),
        all: true,
        scope: SyncScopeArgs {
            state,
            with: Vec::new(),
        },
    };

    let request = args.into_request();

    assert_eq!(request.scope, expected);
    assert!(request.all);
    assert!(request.repositories.is_empty());
}

#[test]
fn included_families_preserve_explicit_selection() {
    let repository: RepositorySelector = "owner/repo".parse().expect("repository selector");
    let args = SyncArgs {
        repositories: vec![repository.clone()],
        all: false,
        scope: SyncScopeArgs {
            state: None,
            with: vec![SyncIncludeArg::Comments, SyncIncludeArg::ReviewThreads],
        },
    };

    let request = args.into_request();

    assert_eq!(request.repositories, vec![repository]);
    assert!(!request.all);
    assert!(request.include_comments);
    assert!(!request.include_reviews);
    assert!(request.include_review_threads);
    assert_eq!(request.parent_run, None);
}
