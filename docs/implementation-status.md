# Implementation status

## Current position

The selected Forgesync implementation is in place. The maintainability cleanup remains
**in progress**: six workflow/presentation batches are implemented, while the cross-crate API,
documentation, and test acceptance review is not yet closed. A passing selected suite or a complete
comment inventory does not establish completion of that review.

This file describes current status. The [source-shape audit](source-shape-audit.md) records review
findings and their dispositions; the [module map](architecture.md) explains the implemented owners.
Earlier milestone-by-milestone development logs are retained in jj history rather than repeated as
current instructions.

## Latest changes and evidence

Scan-start and vector archive methods now explain reserved versus allocated order, cursor origin
validation, partial chunk reads, exact service identity, and independent chunk commits. The audit
records retained scan/lease/document/vector signatures and their existing owners. These comment-only
changes pass nightly formatting; strict documentation validation will run with the final gates.

Coverage persistence now derives acquisition time and sequence from the typed state rather than
duplicate arguments. Parent evidence updates separate sequence conversion from SQL execution. All 16
observation integration cases pass; store all-target Clippy passes. Run/job and audit write
contracts now explain caller validation, transaction ownership, and ledger/coverage limits; their
focused strict Rustdoc check passes. Ordering scenarios have a shallow sibling owner and typed error
assertions; all ten ordering-filtered cases and store Clippy pass for that slice.

Terminal dispatch now uses `QueryDispatch` for one session's borrowed scheduling resources. Its
methods receive action and app rather than seven positional arguments. All 75 TUI unit cases, its
documentation example, all-target Clippy, and strict private-item Rustdoc pass. The workspace
results below cover the preceding child-input API tree; they must be refreshed after this executable
change.

Child acquisition now has three explicit declarations in `forgesync_store::families`:

- `ChildFamilyRequest` names reservation scope and independent source/acquisition clocks.
- `ChildFamilyPage<T>` names one provisional page within an accepted generation.
- `ChildFamilyObservation` declares terminal completeness and optional review-head context.

Reservation and staging accept their declarations plus separate writer authorization when fenced.
Engine collectors and store scenarios construct named fields at each boundary. The archive retains
ordering, replay validation, transaction ownership, and canonical-membership publication.

The latest focused checks pass 26 store observation/search cases, 17 engine sync cases, and three
store documentation examples after reservation changes. The page-input change passes the same 26
store cases and store/engine all-target Clippy; all local workspace tests, Clippy, the minimal CLI
build, and strict public/private Rustdoc pass on that tree. Nightly formatting and changed Markdown
checks pass.

Parent observation cases compare the entire retained discussion after replay, tied conflict, and
malformed-clock rejection. High-water and child publication cases compare exact coverage values.
Review-thread rollback uses distinct old and replacement heads, with a negative control that detects
leaked head context. Engine integration scenarios expose acquisition and archive operations
directly; embedding retry, hybrid ranking, and keyword fallback have independent owners and precise
evidence.

Strict workspace Rustdoc and the minimal CLI build pass on the child-input API tree. Rustdoc
includes private items and denies warnings, missing public docs, and broken links. All 508 workspace
test cases and workspace Clippy also pass on that tree. Provider helper docs now explain
nested-connection completeness, selected head context, and initial URL scope; focused strict
provider Rustdoc passes without executable changes. The refreshed syntax inventory finds no missing
handwritten production function comments or module introductions below ten lines; these checks
establish presence, not documentation quality.

## Bounded remaining work

1. Close the existing cross-crate review inventory. Review unresolved signatures, item/module
   contracts, dispatch, imports, visibility, and scenario clarity. Fix a finding or record a
   concrete reason to retain it; do not turn inspection thresholds into mandatory abstractions.
1. Reconcile the audit, module map, guidance, and completion checklist with the actual code. Remove
   completed findings from the remaining inventory. This status consolidation removes contradictory
   historical next-step notes; broader reconciliation is still open.
1. Run all applicable local gates on the final tree and fix failures attributable to the cleanup.

Stop when each existing requirement has evidence or an explained exception and local gates pass. New
aesthetic opportunities do not expand this cleanup. Preserve source/acquisition ordering, partial
membership, checkpoints, cancellation, fencing, and error isolation throughout the changes.

## Final validation record

Evidence must identify the tree and scope it actually checked. Current focused evidence does not
replace a complete workspace run after the final source edit.

| Gate                            | Current state                                                    |
| ------------------------------- | ---------------------------------------------------------------- |
| Nightly formatting              | Pass on current page-input tree                                  |
| Store/engine all-target Clippy  | Pass on current page-input tree                                  |
| Workspace tests                 | Pass on page-input tree                                          |
| Minimal CLI build               | Pass on current page-input tree                                  |
| Workspace Clippy                | Pass on page-input tree                                          |
| Strict public/private Rustdoc   | Pass on current page-input tree                                  |
| rumdl and changed Markdown lint | Pass for latest edited documentation; rerun after reconciliation |
| Dependency/tool freshness       | Reviewed 2026-09-29; evidence in source-shape audit              |
| Hosted native matrix            | Not run as part of this cleanup                                  |

The final local gates are:

```sh
cargo +nightly fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo build -p forgesync-cli --no-default-features --locked
RUSTDOCFLAGS='-D warnings -D missing_docs -D rustdoc::broken_intra_doc_links' \
  cargo doc --workspace --no-deps --all-features --document-private-items --locked
rumdl check .
markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml <changed-markdown-files>
```

Hosted Linux, Intel macOS, and Windows execution remains separate release evidence. Local checks and
workflow syntax validation cannot establish those results. This cleanup has not published changes or
started hosted execution.

## Implementation milestones

The table retains previously recorded implementation acceptance. It does not certify the current
maintainability pass or replace current-tree validation.

| Task                                       | Status                             | Evidence or next gate                                                                                            |
| ------------------------------------------ | ---------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| P0.1 — Capture the baseline                | Complete                           | Ledger, command migration table, data families, selected matrix                                                  |
| P0.2 — Bootstrap the workspace             | Complete                           | Core/store/CLI, pinned toolchain, lockfile, CI, Clap envelope and process checks                                 |
| P0.3 — Build the fixture catalog           | Complete                           | Named sanitized selected-family scenarios, truth table, loader validation                                        |
| P1.1 — Core identities/outcomes            | Complete                           | Checked identities, normalized content, timestamps, coverage, observations, outcomes                             |
| P1.2 — Explicit SQLite lifecycle           | Complete                           | Exclusive create, explicit migration, read-only/write pools, health checks, CLI commands                         |
| P1.3 — Observation transactions            | Complete                           | Sequence, staging, comparator, membership, and coverage atomicity                                                |
| P1.4 — Offline inspect/search              | Complete                           | Read-only queries, FTS5, stable ties and versioned JSON                                                          |
| P2.1 — HTTP transport and credentials      | Complete                           | Retry, origin-safe auth, cancellation, credential discovery                                                      |
| P2.2 — Thread enumeration                  | Complete                           | Stable identities, durable page cursors, rename handling, replay and partial-failure checks                      |
| P2.3 — Runs, leases, and basic sync        | Complete                           | Fenced writes, run reports, cancellation/replay, closed-sweep overlap                                            |
| P2.4 — Comments and independent failures   | Complete                           | Paginated comments, stale coverage, isolated failures, selective retry                                           |
| P3.1 — PR metadata and reviews             | Complete                           | Head-bound review coverage, normalized reviewer identity, independent failure/retry                              |
| P3.2 — Review threads                      | Complete                           | Typed GraphQL pagination, nested completeness, head-bound state and rollback                                     |
| P3.3 — Legacy import                       | Deferred                           | Reserved task; keep the Go archive intact                                                                        |
| P3.4 — Health and explicit retry           | Complete                           | Read-only diagnostics and selected failed-family retry                                                           |
| P4.1 — Versioned documents                 | Complete                           | Two recipes, timestamp-independent hashes, fenced document persistence                                           |
| P4.2 — Embeddings                          | Complete                           | Bounded client, deterministic chunk batches, validated vectors, retry reuses successful batches                  |
| P4.3 — Semantic and hybrid search          | Complete                           | Paged exact cosine, RRF provenance, explicit fallback, 10k/100k latency and peak-RSS measurements                |
| P4.4 — Clustering and maintainer decisions | Complete                           | Bounded deterministic graph, durable stable IDs, local decisions, partial-coverage safety                        |
| P4.5 — Refresh composition                 | Complete                           | Optional shared-engine stages, durable partial reports, explicit model selection                                 |
| P5.1 — Read-only browser                   | Complete                           | Responsive background queries, repository/thread detail, local search, coverage and failure views                |
| P5.2 — Maintainer actions/live progress    | Complete                           | Engine-backed cluster decisions, sync/retry/refresh, non-blocking progress, and writer ownership                 |
| P6.1 — V2 scope and packaging              | Implemented; hosted matrix pending | Local Apple Silicon smoke/package pass; collect native Linux, Intel macOS, and Windows CI results before release |

A task is complete only when its acceptance checks pass. Keep deferred capabilities absent from
code, workspace members, runtime dependencies, command help, and schema.

## Recorded performance evidence

The following measurement belongs to the semantic-search implementation milestone; it is not a new
benchmark run of the current cleanup tree.

The reproducible exact-cosine benchmark uses the production cosine and ranking implementation with
deterministic 1536-dimensional f32 vectors. Run
`cargo build --release -p forgesync-engine --example exact-cosine-benchmark --locked`, then
`/usr/bin/time -l target/release/examples/exact-cosine-benchmark 10000 1536` and repeat with
`100000`. On an Apple M2 Max with 64 GiB RAM, macOS 26.6.2, and rustc 1.98.1, ranking plus top-20
sorting took 20.192 ms and 204.010 ms, respectively. Peak process RSS was 69,173,248 bytes at 10k
and 636,764,160 bytes at 100k; each vector set contains 61,440,000 and 614,400,000 raw input bytes.
Generation and startup are excluded from the reported ranking time but included in peak RSS. This
measures in-memory cosine ranking and sorting, not SQLite reads or embedding-provider latency.
