# Implementation status

## Current position

The selected Forgesync implementation and maintainability cleanup are **complete**. All eight
implementation/review batches have fixed or retained dispositions, and all applicable local gates
pass on the final source. The full workspace run passes 525 tests across 27 suites, with no failures
or ignored cases. Hosted native execution remains separate release evidence.

This file describes current status. The [source-shape audit](source-shape-audit.md) records review
findings and their dispositions; the [module map](architecture.md) explains the implemented owners.
Earlier milestone-by-milestone development logs are retained in jj history rather than repeated as
current instructions.

## Latest changes and evidence

Sync API, run coordination, and shared acquisition state now have shallow sibling owners. The root
is 162 lines; collectors import state from its defining module. Failure recording belongs to the
thread scope, progress publication to the run context, and current job totals to the work summary.
All seventeen sync integration cases and engine Clippy pass after these changes.

Public discovery no longer exposes an unused seven-argument scoped API. Fenced sync keeps its
private reserved-context executor. Both enumeration cases and engine Clippy pass. Chunk reuse
derives count from selected inputs and keeps unmatched chunks directly; eleven local cases, durable
retry, and engine Clippy pass.

Store coverage derives indexed coordinates from typed state. Generation contracts distinguish
membership removal from local decision transfer, and SQL binding chains separate conversions,
execution, and missing-row checks. All eighty store cases and store Clippy pass. Child reservation,
provisional page, and terminal declarations retain separate ordering/completeness meanings.

The final import pass removes parent-prelude borrowing in store embedding reads and core identity
leaves. Embedding docs distinguish one accepted chunk from complete retrieval evidence. The prior
source passed 519 tests across 27 suites. The final presentation change replaces a behavioral focus
flag with `PaneEmphasis`, preserving cyan/dark-gray borders with six direct cases. All 81 TUI tests,
workspace Clippy, and nightly formatting pass on this source. Selection variants and four fixture
helpers now explain their contracts.

## Remaining work

No work remains in this maintainability cleanup. The source-shape audit records explicit retained
exceptions rather than future cleanup tasks. Before a release, collect the configured hosted native
matrix results; this local cleanup does not certify Linux, Intel macOS, or Windows execution.

## Final validation record

Final acceptance was completed on 2026-09-30 after the last Rust edit. Source, manifests, lockfile,
toolchain, and formatter configuration remained unchanged through acceptance. The 374-file source
fingerprint is SHA-256 `7c1a22c7a6657e63874f01ac14d665e4d1864d85da3bae67219de9ec89317865`.

| Gate                            | Current state                                            |
| ------------------------------- | -------------------------------------------------------- |
| Nightly formatting              | Pass on final presentation source                        |
| Workspace Clippy                | Pass on final presentation source                        |
| Workspace tests                 | Pass: 525 cases, 27 suites, no failures or ignored cases |
| Minimal CLI build               | Pass on final presentation source                        |
| Strict public/private Rustdoc   | Pass on final presentation source                        |
| rumdl and changed Markdown lint | Pass after final record reconciliation                   |
| Dependency/tool freshness       | Reviewed 2026-09-29; evidence in source-shape audit      |
| Hosted native matrix            | Separate release evidence; not run in this cleanup       |

The final local gates are:

```sh
cargo +nightly fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo build -p forgesync-cli --no-default-features --locked
RUSTDOCFLAGS='-D warnings -D missing_docs -D rustdoc::broken_intra_doc_links' \
  cargo doc --workspace --no-deps --all-features --document-private-items --locked
rumdl fmt --check .
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
