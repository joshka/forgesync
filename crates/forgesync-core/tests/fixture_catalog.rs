use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const REQUIRED_INVARIANTS: &[&str] = &[
    "delayed_observations",
    "failure_isolation",
    "closed_sweep_offline_coverage",
    "partial_sync_retains_work",
    "comment_reuse",
    "review_membership_restore",
    "deterministic_cluster_scoring",
];

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn read_json(root: &Path, relative_path: &str) -> Value {
    let path = root.join(relative_path);
    let contents = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read fixture {}: {error}", path.display()));
    serde_json::from_str(&contents)
        .unwrap_or_else(|error| panic!("parse fixture {}: {error}", path.display()))
}

fn collect_json_files(directory: &Path, files: &mut BTreeSet<PathBuf>) {
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read fixture directory {}: {error}", directory.display()));

    for entry in entries {
        let entry = entry.expect("read fixture directory entry");
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, files);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            files.insert(path);
        }
    }
}

fn strings_at<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a str> {
    value[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

#[test]
fn fixture_catalog_validates_every_json_file_and_reference() {
    let root = repository_root();
    let fixture_root = root.join("fixtures");
    let mut json_files = BTreeSet::new();
    collect_json_files(&fixture_root, &mut json_files);
    assert!(!json_files.is_empty(), "fixture catalog must not be empty");

    for path in &json_files {
        let contents = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("read fixture {}: {error}", path.display()));
        let _: Value = serde_json::from_str(&contents)
            .unwrap_or_else(|error| panic!("parse fixture {}: {error}", path.display()));
        let text = contents.to_ascii_lowercase();
        assert!(
            !text.contains("ghp_"),
            "fixture {} contains a token-like value",
            path.display()
        );
        assert!(
            !text.contains("authorization"),
            "fixture {} contains an auth header",
            path.display()
        );
        assert!(
            !text.contains("access_token"),
            "fixture {} contains a credential field",
            path.display()
        );
    }

    let catalog = read_json(&root, "fixtures/scenarios/catalog.json");
    assert_eq!(catalog["format_version"].as_u64(), Some(1));
    assert_eq!(
        catalog["truth_table"].as_str(),
        Some("fixtures/scenarios/observation_ordering.json")
    );

    let mut referenced_files = BTreeSet::new();
    for path in strings_at(&catalog, "fixtures") {
        assert!(
            path.starts_with("fixtures/"),
            "fixture path must stay under fixtures/: {path}"
        );
        assert!(
            !path.contains(".."),
            "fixture path must not traverse directories: {path}"
        );
        let fixture = root.join(path);
        assert!(
            fixture.is_file(),
            "catalog references missing fixture {path}"
        );
        referenced_files.insert(fixture);
    }

    let provider_fixtures = json_files
        .iter()
        .filter(|path| path.starts_with(fixture_root.join("github")));
    for path in provider_fixtures {
        assert!(
            referenced_files.contains(path),
            "unreferenced provider fixture {}",
            path.display()
        );
    }

    let scenarios = catalog["scenarios"].as_array().expect("scenarios array");
    assert!(
        !scenarios.is_empty(),
        "catalog must contain named scenarios"
    );
    let mut scenario_ids = BTreeSet::new();
    let mut covered_invariants = BTreeSet::new();
    for scenario in scenarios {
        let id = scenario["id"].as_str().expect("scenario id");
        assert!(scenario_ids.insert(id), "duplicate scenario id {id}");
        assert!(
            !strings_at(scenario, "reference_tests")
                .collect::<Vec<_>>()
                .is_empty(),
            "scenario {id} needs source test names"
        );
        let fixture_paths: Vec<_> = strings_at(scenario, "fixtures").collect();
        assert!(!fixture_paths.is_empty(), "scenario {id} needs fixtures");
        for path in fixture_paths {
            assert!(
                referenced_files.contains(&root.join(path)),
                "scenario {id} references unlisted fixture {path}"
            );
        }
        for invariant in strings_at(scenario, "invariants") {
            assert!(
                REQUIRED_INVARIANTS.contains(&invariant),
                "unknown invariant {invariant} in scenario {id}"
            );
            covered_invariants.insert(invariant);
        }
    }

    let declared_invariants: BTreeSet<_> = strings_at(&catalog, "required_invariants").collect();
    let required_invariants: BTreeSet<_> = REQUIRED_INVARIANTS.iter().copied().collect();
    assert_eq!(
        declared_invariants, required_invariants,
        "catalog invariant list differs from selected regression matrix"
    );
    assert_eq!(
        covered_invariants, required_invariants,
        "each selected invariant must map to a named scenario"
    );
}

#[test]
fn observation_truth_table_has_named_rows_for_each_ordering_case() {
    let root = repository_root();
    let truth_table = read_json(&root, "fixtures/scenarios/observation_ordering.json");
    assert_eq!(truth_table["format_version"].as_u64(), Some(1));
    let cases = truth_table["cases"].as_array().expect("truth table cases");
    assert!(
        !cases.is_empty(),
        "observation truth table must not be empty"
    );

    let mut ids = BTreeSet::new();
    let mut cases_by_source = BTreeMap::<&str, usize>::new();
    for case in cases {
        let id = case["id"].as_str().expect("truth table case id");
        assert!(ids.insert(id), "duplicate truth table case {id}");
        assert!(
            case["operation"].as_str().is_some(),
            "truth table case {id} needs an operation"
        );
        assert!(
            case.get("expected").is_some(),
            "truth table case {id} needs an expected outcome"
        );
        let source_test = case["source_test"].as_str().expect("source test name");
        *cases_by_source.entry(source_test).or_default() += 1;
    }

    for source_test in [
        "TestCompareObservationOrder",
        "TestCompareRevisionObservationOrder",
        "TestThreadChildObservationReservationsAdvanceIndependently",
        "TestUpsertThreadObservationRejectsDelayedCanonicalOverwrite",
        "TestUpsertThreadObservationIsIdempotentButRejectsTiedConflicts",
        "TestUpsertThreadObservationTracksIncompleteEvidenceGeneration",
        "TestUpsertThreadObservationRejectsDelayedIntermediateParentAfterIncompleteReplay",
        "TestUpsertThreadObservationCompletionPreservesGenerationHighWaterMark",
        "TestUpsertThreadObservationCompletesNewerSourceBelowEvidenceSequence",
        "TestUpsertThreadObservationStartsIncompletePayloadWithoutEvidenceGeneration",
        "TestUpsertThreadObservationUsesSequenceWithoutSourceTimestamp",
        "TestUpsertThreadObservationRejectsAmbiguousMalformedClocks",
        "TestObservationSequenceOrderValueHandlesMinInt64",
    ] {
        assert!(
            cases_by_source.contains_key(source_test),
            "truth table omits named source case {source_test}"
        );
    }
}
