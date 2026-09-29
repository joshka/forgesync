# Source shape audit

This inventory records the command ownership, function documentation, and dispatch review across all
six crates. It distinguishes a large match that hides behavior from a compact match that states
policy. Keep this as maintenance evidence; [Rust conventions](rust-conventions.md) holds the
reusable rules.

| Crate  | Finding                                                                                                                                                                                            | Decision                                                                                                                                                                   |
| ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| CLI    | Parallel `args/` and `commands/` trees split every command type from its execution. Archive, thread, and run dispatchers nested archive I/O and rendering inside their arms.                       | Consolidated into `command/`. Parsed types own `run`; dispatch points to those methods. Shared provider setup, retry, output, and configuration retain their own owners.   |
| TUI    | `App::apply` mixed generation checks, result handling, and state mutation in one message match. Browser key handling mixed navigation and query construction in several arms.                      | Extracted one result method per message. Browser and triage keys now dispatch to named actions; selection and query effects have local owners.                             |
| Engine | `retrieve_threads` held keyword, semantic, hybrid, and fallback workflows inside one match. Sync and refresh coordinate archive, clients, requests, and cancellation without one natural receiver. | Extracted search paths and fallback. Keep sync and refresh as workflow functions; review large phase helpers on their own merits.                                          |
| Store  | Archive lifecycle and storage operations already live on `Archive`. Observation application is a long, correctness-sensitive transaction; ordering matches express the replacement policy.         | Retain the `Archive` receiver and ordering matches. Documented helper contracts; review observation phases separately while preserving transaction and completeness rules. |
| GitHub | Transport behavior is on `GitHubClient`. Resource fetch and normalization functions are grouped by provider family; pagination and retry matches are mostly local policy.                          | Keep resource families visible instead of flattening them into one client implementation. Documented scope, pagination, and failure helper contracts.                      |
| Core   | Checked identities and values already own their validation methods. Remaining matches are small value or domain-state mappings.                                                                    | Keep current ownership and document private identity and coverage helpers.                                                                                                 |

## Follow-up audit

The initial ownership pass addressed selected workflows in every crate; it did not establish that
all functions, APIs, and test bodies had received the same review. Completion claims must name the
reviewed surface and its evidence, rather than extrapolating from a crate or area label.

Implemented follow-ups:

- Browser keys dispatch to named actions. `Movement` owns bounded position calculations; each pane
  owns selection, invalidation, and scrolling effects.
- Embedding reuse and replacement use `EmbeddingPolicy`. Refresh materialization has a stage owner
  with repository traversal, document outcomes, and vector batch accounting.
- Search input and cluster setup tests spell out their small scenarios without loops.
- Semantic search uses `SemanticSource` for candidate scope and `SemanticRanking` for bounded
  scores. Candidate availability precedes query embedding; every page uses the same filter scope.
- Changed browser, refresh embedding, and semantic modules import dependencies at their owners.
- Archive diagnostics separate schema validation, lease observation, and durable work queries.
- Reservation uses `ReservedGeneration` for ordering and persistence; staging uses `PageWrite` for
  generation validation, replay comparison, insertion, and count accounting. Commit remains with the
  archive operation.
- Family reuse uses `MembershipExpectation` and `FamilyFreshness` for source clock, head, coverage,
  and membership evidence. No behavioral bool selects its validation policy.
- Global, search, failure, cluster, and member input dispatch names the action instead of performing
  multi-step state changes inside key-match arms.
- Reservation APIs describe ordering, rejected generations, fencing, errors, and canonical effects.
- Semantic-search docs distinguish archived document vectors from the network-generated query
  vector.

Remaining review surfaces:

- Broader function and state review beyond the selected traversal and diagnostic slices.
- Review additional store operations beyond the reservation, staging, and freshness slices.
- Restricted visibility and imports routed through parent module aliases, including the engine
  root's passthrough exports of store DTO collections.
- Deeper module and item documentation contracts across all crates. All six crate introductions now
  have expanded entry guidance and have been reviewed in rendered Rustdoc.
- Hosted behavior of the refreshed CI actions. Local workflow syntax validation passes.
- Hosted Linux, Intel macOS, and Windows validation; local checks cannot establish those results.

Retain simple domain mappings and linear SQL binding maps when splitting them increases navigation.
Review exceptions on their actual contracts rather than using line counts as proof of completion.

## Completion checklist

These requirements preserve the full maintainer request. A passing compiler or a selected slice is
not evidence for every row. Keep this checklist open until its scope has actually been reviewed.

| Requirement                                              | Current evidence                                                         | Remaining work                                                                                   |
| -------------------------------------------------------- | ------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------ |
| Meaningful small modules, broad shallow navigation       | Six crate module maps and selected vertical slices                       | Inspect current long functions and multi-effect match arms throughout the workspace              |
| Command/state ownership and top-down reading             | Unified CLI command tree; workflow and transaction owners                | Review remaining CLI and engine orchestration, TUI result application, and presentation branches |
| Domain types for related inputs; no behavioral bools     | Embedding policy and family membership expectations                      | Review every remaining bool parameter and broad signature for an intentional contract            |
| Explicit local imports and deliberate visibility         | No `use super::*`; changed workflow imports name owners                  | Remove remaining parent import preludes and document public-boundary exceptions                  |
| Every application function documented                    | Syntax inventory found two missing production comments; both corrected   | Recheck after subsequent changes; assess depth rather than presence alone                        |
| All modules and items teach their role and relationships | Expanded roots, workflow modules, examples, and API contracts            | Review remaining private type/constant docs, field contracts, module maps, and rendered pages    |
| Linear nearby tests with clear scenarios                 | Split suites and direct keyboard/cluster setup                           | Inspect remaining scenario branches and fixture burden; preserve meaningful data-driven cases    |
| Current dependencies and tools                           | Full direct/transitive aggressive audit reports no outdated dependencies | CI actions refreshed against upstream; native tools checked and nightly refreshed                |
| Reusable guidance recorded                               | Linked documentation and Rust conventions guides                         | Record any additional recurring findings at the owning guide                                     |
| Formatting and local gates                               | Previous follow-up passed every local gate                               | Rerun focused and workspace gates for each subsequent implementation batch                       |
| Hosted platform evidence                                 | Native smoke/package matrix is configured                                | Obtain current Linux, Intel macOS, and Windows execution results                                 |

The syntax inventory distinguishes production functions from tests and trait implementations. It
measures actual function bodies, excluding braces in strings. The initial continuation found 38
production functions over 50 body lines and 113 match arms spanning at least five lines. These are
inspection candidates, not defects by themselves: simple error-code tables, DTO construction, and
linear SQL binding maps may remain. The item pass also found undocumented private representations
and policy constants; trait-associated aliases inherit their trait contract. Documentation presence
and line counts do not establish documentation quality.

### Current tool evidence

Checked on 2026-09-29: rumdl 0.2.77 matches the
[upstream release](https://github.com/rvben/rumdl/releases/tag/v0.2.77); actionlint 1.7.12 matches
[its upstream release](https://github.com/rhysd/actionlint/releases/tag/v1.7.12).
The npm registry reports markdownlint-cli2 0.23.3, matching the installed command. Nightly is
refreshed to 2026-09-29, with rustc 1.101.0-nightly and its matching rustfmt. The repository's Rust
1.98.1 toolchain remains its deliberate build and validation baseline.

CI and release workflows now use checkout v7, setup-python v7, upload-artifact v7, and
download-artifact v8. The upstream usage and input contracts preserve this repository's checkout,
Python setup, and zipped artifact transfer:
[checkout](https://github.com/actions/checkout/blob/v7/README.md),
[setup-python](https://github.com/actions/setup-python/blob/v7/README.md),
[upload-artifact](https://github.com/actions/upload-artifact/blob/v7/README.md), and
[download-artifact](https://github.com/actions/download-artifact/blob/v8/README.md).
Actionlint passes both workflow files. Hosted execution remains separate evidence.

### Next concrete review targets

- Remove engine-root passthrough exports and route child imports through their actual owners.
- Document remaining private representations, policy constants, and TUI state fields with their
  contracts; inspect short module introductions for missing relationships rather than adding words.
- Review CLI search/retry result branches, TUI result application, refresh cluster traversal,
  embedding batch scheduling, and sync run coordination against the dispatch and state-owner rules.
- Replace the outcome serialization and invalid-reference test loops with named cases, and write
  enumeration replay scenarios linearly. Review catalog-validation loops separately: checking a
  complete fixture catalog is a different contract from selecting multiple behavioral scenarios.

These targets are entries into the full checklist above, not a replacement or narrower definition of
completion. The final current-tree local gates pass for this batch; hosted execution and the
remaining semantic review are still required evidence.
