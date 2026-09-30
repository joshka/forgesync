# Implementation status

## Current position

The selected Forgesync implementation and maintainability cleanup are **complete**. All eight
implementation/review batches have fixed or retained dispositions, and all applicable local gates
passed on the cleanup baseline. That workspace run passed 525 tests across 27 suites, with no
failures or ignored cases. The follow-ups below have separate validation evidence, including the
initial registry release and hosted native execution.

This file describes current status. The [source-shape audit](source-shape-audit.md) records review
findings and their dispositions; the [module map](architecture.md) explains the implemented owners.
Earlier milestone-by-milestone development logs are retained in jj history rather than repeated as
current instructions.

## Diagnostic colors follow-up

Text diagnostics enable tracing's default ANSI colors and respect nonempty `NO_COLOR`. JSON logs
remain uncolored. Process checks verify those behaviors and unchanged stdout. Validation in the
isolated `work/tracing-colors` workspace passes: 31 CLI contract tests, 542 workspace tests across
29 suites, nightly formatting, workspace Clippy, the CLI-only build, Rustdoc, and Markdown lint.
The next action is PR review and workspace cleanup after merge; no implementation work remains.

## Registry and trusted release follow-up

The public `joshka/forgesync` repository and MIT OR Apache-2.0 licensing are configured. All seven
packages have descriptions, repository links, READMEs, and packaged license texts. The release-plz
workflow uses GitHub OIDC in the main-only `crates-io` environment and validates generated release
PRs through reusable CI. Release-plz configuration schema and workflow linting pass.

All seven crates are published at 0.1.0 with registry checksums matching the verified local release
archives. Each trusted publisher entry is read back through the crates.io API and matches
`joshka/forgesync`, `release-plz.yml`, and `crates-io`. Source and all seven bootstrap tags are
pushed. A crates.io installation with `--locked`, default features, and the debug profile passes;
its binary includes terminal browsing and passes the credential-free offline smoke script.

Hosted Rust gates and all four platform smoke jobs pass in the release workflow. The publisher
correctly skips a non-release-PR commit, and release-plz reports no version changes after bootstrap.
Its empty PR output is `{}`, so the reusable PR check now depends on an actual returned branch
rather than a nonempty JSON string. The corrected
[release workflow](https://github.com/joshka/forgesync/actions/runs/36762278832) passes and skips PR
checks when no release PR exists. The corresponding
[CI run](https://github.com/joshka/forgesync/actions/runs/36762278056) also passes. Future OIDC
uploads occur when a reviewed release PR is merged; bootstrap publication used local Cargo
credentials.

## Product facade follow-up

The `forgesync` package now owns the installable executable and module-oriented Rust facade. Its CLI
and TUI features are enabled by default. The CLI package is a library; process tests live with the
executable. Workflow implementations remain in the existing core, store, provider, and engine
owners. CI and release builds target the product, and workspace path dependencies carry registry
version requirements so publication can preserve the same graph. The registry follow-up above
records subsequent publication.

Validation passes: 539 workspace tests across 29 suites, with no failures or ignored cases;
workspace Clippy; strict public/private Rustdoc; nightly formatting; and Markdown checks. The
product-focused run passes 47 process tests plus its facade example. Library-only checking excludes
CLI and TUI dependencies; the CLI-only build omits browsing. A temporary local default-feature
installation exposes browsing and passes the credential-free smoke script. The registry follow-up
above records subsequent publication and hosted validation.

## Configured-archive follow-up

The CLI now selects an invocation override, `[archive] path`, or one default user-data database.
User configuration is discovered automatically; relative TOML paths are anchored to that file.
Explicit initialization creates missing database parent directories. Reads and migration remain
existing-file operations. TUI terminal/output validation precedes config loading, then interactive
startup uses the same database resolution as other commands.

The default workflow and migration from previously explicit paths are documented in the README,
configuration guide, and user manual. The manual distinguishes SQLite lock waiting, provider and
credential timeouts, and workflow writer leases; the archive-wide lease design is retained.

CLI validation passes: 129 cases across six suites, including five new path-resolution and eight
process-selection cases. Workspace Clippy and strict public/private Rustdoc pass. The native smoke
script passes with automatic configuration and no archive flags. Formatting and Markdown checks
pass. The full workspace regression run passes 538 tests across 27 suites, with no failures or
ignored tests. The minimal CLI build passes with default features disabled; it ran after process
tests completed to avoid replacing their executable. The cleanup validation record below certifies
its earlier baseline, rather than this changed application source.

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

No work remains in this maintainability cleanup or the initial registry release setup. The
source-shape audit records explicit retained exceptions rather than future cleanup tasks. The
registry follow-up records hosted validation; the cleanup baseline below records its earlier local
acceptance scope.

## Cleanup baseline validation record

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

| Task                                       | Status                             | Evidence or next gate                                                                             |
| ------------------------------------------ | ---------------------------------- | ------------------------------------------------------------------------------------------------- |
| P0.1 — Capture the baseline                | Complete                           | Ledger, command migration table, data families, selected matrix                                   |
| P0.2 — Bootstrap the workspace             | Complete                           | Core/store/CLI, pinned toolchain, lockfile, CI, Clap envelope and process checks                  |
| P0.3 — Build the fixture catalog           | Complete                           | Named sanitized selected-family scenarios, truth table, loader validation                         |
| P1.1 — Core identities/outcomes            | Complete                           | Checked identities, normalized content, timestamps, coverage, observations, outcomes              |
| P1.2 — Explicit SQLite lifecycle           | Complete                           | Exclusive create, explicit migration, read-only/write pools, health checks, CLI commands          |
| P1.3 — Observation transactions            | Complete                           | Sequence, staging, comparator, membership, and coverage atomicity                                 |
| P1.4 — Offline inspect/search              | Complete                           | Read-only queries, FTS5, stable ties and versioned JSON                                           |
| P2.1 — HTTP transport and credentials      | Complete                           | Retry, origin-safe auth, cancellation, credential discovery                                       |
| P2.2 — Thread enumeration                  | Complete                           | Stable identities, durable page cursors, rename handling, replay and partial-failure checks       |
| P2.3 — Runs, leases, and basic sync        | Complete                           | Fenced writes, run reports, cancellation/replay, closed-sweep overlap                             |
| P2.4 — Comments and independent failures   | Complete                           | Paginated comments, stale coverage, isolated failures, selective retry                            |
| P3.1 — PR metadata and reviews             | Complete                           | Head-bound review coverage, normalized reviewer identity, independent failure/retry               |
| P3.2 — Review threads                      | Complete                           | Typed GraphQL pagination, nested completeness, head-bound state and rollback                      |
| P3.3 — Legacy import                       | Deferred                           | Reserved task; keep the Go archive intact                                                         |
| P3.4 — Health and explicit retry           | Complete                           | Read-only diagnostics and selected failed-family retry                                            |
| P4.1 — Versioned documents                 | Complete                           | Two recipes, timestamp-independent hashes, fenced document persistence                            |
| P4.2 — Embeddings                          | Complete                           | Bounded client, deterministic chunk batches, validated vectors, retry reuses successful batches   |
| P4.3 — Semantic and hybrid search          | Complete                           | Paged exact cosine, RRF provenance, explicit fallback, 10k/100k latency and peak-RSS measurements |
| P4.4 — Clustering and maintainer decisions | Complete                           | Bounded deterministic graph, durable stable IDs, local decisions, partial-coverage safety         |
| P4.5 — Refresh composition                 | Complete                           | Optional shared-engine stages, durable partial reports, explicit model selection                  |
| P5.1 — Read-only browser                   | Complete                           | Responsive background queries, repository/thread detail, local search, coverage and failure views |
| P5.2 — Maintainer actions/live progress    | Complete                           | Engine-backed cluster decisions, sync/retry/refresh, non-blocking progress, and writer ownership  |
| P6.1 — V2 scope and packaging              | Implemented; hosted matrix pending | Initial hosted platform checks passed; native assets and platform checks are now opt-in           |

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

## CI cache investigation

The isolated `ci-cache` workspace adds opt-in Linux cache measurements to the manual Platform checks
workflow. Routine CI and publication remain unchanged. Hosted cold/warm runs pass for compilation,
Clippy, docs, default and CLI-only smoke, workspace tests, and focused process tests. Combined
Clippy plus default-binary smoke finishes in 47 seconds for a complete warm workflow; focused
CLI/offline tests finish in 37 seconds. Full workspace tests take 74 seconds complete warm and
exceed the selected budget. Cold job costs range from 79 to 140 seconds across measured lanes.

See the [experiment report](ci-cache-experiment.md) and [timing evidence](ci-cache-timings.json) for
commands, revisions, step costs, cache invalidation, coverage limits, and the proposed gate.
Actionlint and changed Markdown lint pass. The next task is to review the combined PR gate and
repeat measurements with representative source/dependency changes before changing routine CI. Tokio
signal/subprocess regressions are supplied separately by the merged
[runtime-driver fix](https://github.com/joshka/forgesync/pull/2).

## Rapid release follow-up

Routine hosted checks temporarily compile the workspace on Linux; full platform builds no longer
gate crate publication, and Intel macOS is removed. Stronger checks and measured cache experiments
are tracked in [the CI follow-up issue](https://github.com/joshka/forgesync/issues/1). A separate
task fixes the reported CLI sync panic caused by missing Tokio runtime drivers; compilation alone
does not verify that runtime boundary.

## CLI runtime driver follow-up

The process runtime now enables Tokio's I/O, signal, and timer drivers. Inspection found one manual
runtime builder; TUI operations borrow the process runtime. Two synchronous regressions use that
same builder to register Ctrl-C and execute a harmless subprocess with captured stdout. Restoring
the timer-only builder makes both tests reproduce the reported Tokio panics. A Unix process test
invokes `sync ratatui/ratatui` with an isolated `gh` fixture that emits malformed, credential-free
output; the typed credential error stops execution before any GitHub request.

Local validation on 2026-09-30 passes: both runtime regressions, all five sync process cases, the
full workspace suite (542 unit, integration, and documentation tests), workspace Clippy with warnings
denied, nightly formatting, the minimal CLI build, workspace Rustdoc, and changed Markdown lint.
The process fixture initially exceeded its ten-second startup allowance during cold compilation;
it now allows thirty seconds, and the final workspace run passes. Unix process coverage ran on
macOS; the portable runtime tests have not been executed on Windows in this task.

The next action is integration and release by the parent task; this workspace does not publish.

## Runner image stability

All Linux workflow jobs pin `ubuntu-24.04`, including routine CI and release-plz, to avoid the
automatic `ubuntu-latest` migration to Ubuntu 26.04. Actionlint passes for all workflows. The next
CI task remains the measured coverage and cache work tracked in
[the CI follow-up issue](https://github.com/joshka/forgesync/issues/1).

## Dependency maintenance

The 2026-09-30 registry check replaces yanked `yoke-derive 0.8.3` with `0.8.4` and refreshes
`quinn-proto` to `0.11.19` and `quinn-udp` to `0.5.16`. `yoke 0.8.3` is not yanked. All direct
dependencies and workflow actions already resolve to their latest stable releases. Cargo retains
`crypto-common 0.1.6` with `generic-array 0.14.9`; forcing `crypto-common 0.1.7` requires an exact
`generic-array 0.14.7`, and a normal update restores Cargo's preferred pair.

Dependabot checks Cargo and GitHub Actions weekly, groups each ecosystem's updates, and applies a
seven-day cooldown. Cargo uses `increase-if-necessary` so compatible updates preserve manifest
requirements while newer incompatible releases can propose requirement changes. Existing security
updates remain enabled and are exempt from the version-update cooldown.

Local validation passes: 41 focused process cases, all 542 workspace unit/integration/documentation
cases, Clippy with warnings denied, nightly formatting, the CLI-only build, Rustdoc, workflow lint,
and Dependabot YAML checks. Cargo outdated reports no pending workspace dependency updates; Cargo
audit with `--deny yanked` reports no vulnerabilities or yanked packages. The next maintenance
action is to review the scheduled Dependabot PRs; the CI follow-up remains tracked separately.
