# Implementation status

## Current position

- Next task: **P2.1 — HTTP transport and credentials**.
- Complete: **P0.1 — Capture the baseline and reconcile selected v2 scope**.
- Complete: **P0.2 — Bootstrap the Rust workspace**.
- Complete: **P0.3 — Build the fixture catalog**.
- Complete: **P1.1 — Core identities and outcomes**.
- Complete: **P1.2 — Explicit SQLite lifecycle**.
- Complete: **P1.3 — Observation transactions**.
- Reference checkout: /Users/joshka/local/gitcrawl/default.
- Reference change: ymmxytsluuktnvuwqqmrrsoqqmtyvpls.
- Reference commit: 8c9a4f85b7c4eaae5b7d279c2e83c2eb167bed3a.
- Reference archive schema: version 13.
- Forgesync repository: /Users/joshka/local/forgesync, initialized as non-colocated jj.
- The Go checkout and production archives were not modified.

## P0.1 evidence

Read the revised plan's Selected v2 scope, crate/module map, contracts, CLI audit and proposed command
tree, staged tasks, regression matrix, and execution guidance. Read the implementation handoff and
reference checkout instructions. Inspected Gitcrawl's README/SPEC, command/config/sync/search/
clustering/governance/review-thread/TUI docs, CLI dispatch, schema and migrations, selected source
implementations/tests, and CrawlKit v0.16.5 remote/config/store/snapshot/progress/vector interfaces.

docs/compatibility.md contains the requested compact feature disposition ledger, the CLI migration
table, every dispatched/documented command surface, all current Go table families, and the selected
regression matrix. It explicitly defers cloud/portable distribution, source indexing, summaries,
metrics/analytics, owner erasure, full revision history, legacy import, deep PR details, and old CLI
compatibility. It distinguishes local triage choices from GitHub state.

The observation tests show that the top-level comparator is only part of the contract. The P0.3
truth table records source-clock ordering, revision acquisition ordering, incomplete generations,
independent family reservations, parent/child freshness, malformed clocks, idempotence, conflicts,
and membership atomicity. No contradictory example was established.

## P0.2 evidence

Created the resolver-3 workspace with core, store, and CLI crates, pinned Rust 1.98.1, generated
Cargo.lock, added CI checks and the repository-specific AGENTS.md. The current CLI has global Clap
options, --version/help behavior, and a tested versioned JSON envelope. It intentionally advertises
no feature commands until an implementation phase provides their handlers. No Gitcrawl or CrawlKit
source code was copied, so no inherited source-license attribution was needed.

Cargo resolved Clap 4.6.7, Serde 1.0.229, serde_json 1.0.151, and assert_cmd 2.2.2. These are lockfile
versions, not raised minimum requirements.

Validation:

- markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml docs/compatibility.md
  docs/implementation-status.md: passed, 0 issues.
- cargo fmt --all -- --check: passed.
- cargo clippy --workspace --all-targets --all-features --locked -- -D warnings: passed.
- cargo test --workspace --locked: passed, 6 tests.
- cargo build -p forgesync-cli --no-default-features --locked: passed.
- cargo doc --workspace --no-deps --all-features --locked: passed.
- Offline cargo install to target/install-smoke and installed --version/--help smoke: passed.
- Reference jj revision recorded without changing the checkout.

## P0.3 evidence

Added synthetic, credential-free provider fixtures for repositories, issues, ordinary PR base/head
metadata, complete and empty comments, split comment pages, reviews, nested GraphQL review-thread
pages/comments, partial GraphQL errors, rate limiting, and deterministic cluster scoring. Scenario
fixtures cover interrupted acquisition and review-thread visibility, tombstone, and restore behavior.
No real private discussion content or credentials are included.

`fixtures/scenarios/observation_ordering.json` is the named truth table. It records canonical source
clock ordering, revision acquisition ordering, missing and malformed clocks, incomplete generation
high-water marks and later hydration, family-independent reservations, parent/child freshness,
idempotent replay, conflict rejection, complete-empty versus incomplete membership, and atomic review
refresh. Each selected regression invariant in the compatibility matrix maps to at least one named
scenario. Changed-file fixtures and duplicate paths remain absent because deep PR file data is
deferred.

The loader parses every JSON file, verifies catalog paths and provider-fixture references, rejects
credential-like values, checks unique scenario IDs, maps all selected invariants, and confirms that
each named ordering case points to a reference test.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 8 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.
- Focused offline fixture loader run: passed, 2 tests.

## P1.1 evidence

Added core types for canonical GitHub host identity, opaque provider IDs, checked repository/thread/
comment/review/review-thread/run IDs, positive thread numbers and observation sequences, and full
SHA-1/SHA-256 commit IDs. UTC timestamps parse RFC 3339, normalize to integer microseconds, and
serialize as normalized UTC strings. Source clocks retain missing, valid, and invalid states
separately.

Added normalized repository, discussion, pull request base/head, comment, review, and review-thread
content; review evidence carries commit/head context. Unknown provider fields use a sorted JSON object
that preserves nested values. Family coverage distinguishes missing, incomplete, complete (including
empty), unavailable, failed, and deferred. Database-independent observations carry family, payload,
source clock, acquisition time, sequence, and collection completeness. Operation outcomes have
distinct complete, partial, deferred, failed, and interrupted states.

The versioned CLI JSON envelope remains in `forgesync-cli`; core types do not depend on the CLI crate
or its output DTOs. Tests cover rejected identity/time inputs, timezone normalization, archive-time
precision, unknown provider-field round trips, and serialized coverage and outcome states.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 20 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.
- `markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml docs/compatibility.md
  docs/implementation-status.md`: passed, 0 issues.

## P1.2 evidence

Added the SQLite-only SQLx store with an embedded initial migration, Forgesync format identity,
UUID archive identity, and UTC creation time. Creation uses exclusive file creation and refuses to
overwrite an existing path. Read-only and read-write opens require an existing regular file and
reject unsupported, unmigrated, or newer schemas without applying migrations. Migration is a
separate operation. Read connections are read-only; writable handles use a single writer
connection, WAL, FULL synchronous mode, foreign keys, and a five-second busy timeout.

Added `archive init`, `archive migrate`, `archive status`, and `archive doctor`. Status opens the
archive read-only. Doctor runs SQLite quick-check and probes FTS5 and actual foreign-key enforcement
with temporary objects. All four commands have human output and the shared versioned JSON envelope;
errors have stable machine-readable codes.

On-disk tests cover exclusive creation, read-only and read-write open, current-schema migration,
missing paths, refusal of newer, dirty, and checksum-mismatched migration histories, temporary health
probes, FTS5, foreign keys, and CLI JSON results.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 27 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.

## P1.3 evidence

Added a durable archive-wide sequence and separate repository, thread, reservation, staged page,
canonical membership, and per-family coverage records. Parent observations retain separate source
high-water and complete-evidence provenance. Delayed observations cannot replace newer accepted
state, tied conflicting payloads fail, and identical replays are idempotent.

Child-family acquisition now reserves each family independently, stages typed provider items by
page, and atomically applies complete membership with coverage. Incomplete results update coverage
while retaining the last complete membership; complete empty results remove membership. Superseded
generations are skipped, and failed membership/coverage writes roll back together. Added ordering
helpers for valid, missing, malformed, and legacy revision clocks.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 37 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.
- Focused `cargo test -p forgesync-store --all-features --locked`: passed, 15 tests.

## P1.4 evidence

Added a transactional FTS5 index for current thread titles and bodies, including insert, update,
delete, and existing-row backfill triggers. Migration remains explicit; the v2-to-v3 test confirms
read-only open rejects the old schema and that migration indexes existing discussion text.

Added store and engine APIs for archive status, repository-scoped thread lists, keyword and advanced
FTS search, current thread detail, chronological current-content timelines, and per-family coverage.
Queries bind all values, use stable ordering ties and bounded pagination, and run through the
read-only connection. Ordinary search quotes extracted text terms so punctuation and FTS operators
remain text. Explicit advanced syntax reports malformed FTS as a query error. The CLI adds
`search`, `thread list`, and `thread show`; JSON and human output share the same read-only engine
operations.

On-disk and process tests cover filtering, pagination, empty-result coverage, FTS updates and text
removal, explicit migration backfill, typed thread evidence and timeline, malformed advanced query
errors, usage errors, offline execution without GitHub credentials, and unchanged archive status
across reads.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 46 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.
- `markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml docs/compatibility.md
  docs/implementation-status.md`: passed, 0 issues.

## Task sequence

| Task | Status | Evidence or next gate |
| --- | --- | --- |
| P0.1 — Capture the baseline | Complete | Ledger, command migration table, data families, selected matrix |
| P0.2 — Bootstrap the workspace | Complete | Core/store/CLI, pinned toolchain, lockfile, CI, Clap envelope and process checks |
| P0.3 — Build the fixture catalog | Complete | Named sanitized selected-family scenarios, truth table, loader validation |
| P1.1 — Core identities/outcomes | Complete | Checked identities, normalized content, timestamps, coverage, observations, outcomes |
| P1.2 — Explicit SQLite lifecycle | Complete | Exclusive create, explicit migration, read-only/write pools, health checks, CLI commands |
| P1.3 — Observation transactions | Complete | Sequence, staging, comparator, membership, and coverage atomicity |
| P1.4 — Offline inspect/search | Complete | Read-only queries, FTS5, stable ties and versioned JSON |
| P2.1 — HTTP transport and credentials | Next | Retry, origin-safe auth, cancellation, credential discovery |
| P2.2–P2.4 — GitHub acquisition/recovery | Not started | Complete pagination, leases, checkpoints, isolated failures |
| P3.1–P3.4 — Reviews and health | Not started | PR base/head + reviews, review threads, coverage and explicit retry |
| P4.1–P4.5 — Retrieval and analysis | Not started | Versioned documents, embeddings, semantic search, clustering, refresh |
| P5.1–P5.2 — TUI | Not started | Responsive shared-engine browser and maintainer actions |
| P6.1 — V2 scope and packaging | Not started | Release only selected local workflows; deferred scope absent |

A task is complete only when its acceptance checks pass. Keep deferred capabilities absent from code,
workspace members, runtime dependencies, command help, and schema.
