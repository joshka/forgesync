# Implementation status

## Current position

- Next task: **P0.3 — Build the fixture catalog**.
- Complete: **P0.1 — Capture the baseline and reconcile selected v2 scope**.
- Complete: **P0.2 — Bootstrap the Rust workspace**.
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

The observation tests show that the top-level comparator is only part of the contract. P0.3 must
fixture revision freshness, incomplete generations, independent family reservations, source clock
equivalence/malformed cases, and transactional application before P1.3 implementation. No conflict
has been established.

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

## Task sequence

| Task | Status | Evidence or next gate |
| --- | --- | --- |
| P0.1 — Capture the baseline | Complete | Ledger, command migration table, data families, selected matrix |
| P0.2 — Bootstrap the workspace | Complete | Core/store/CLI, pinned toolchain, lockfile, CI, Clap envelope and process checks |
| P0.3 — Build the fixture catalog | Next | Named, sanitized selected-family scenarios and observation truth table |
| P1.1 — Core identities/outcomes | Not started | Checked host/repository/thread/run IDs and typed evidence states |
| P1.2 — Explicit SQLite lifecycle | Not started | Separate create/open/migrate; no open-time creation or migration |
| P1.3 — Observation transactions | Not started | Sequence, staging, comparator, membership, and coverage atomicity |
| P1.4 — Offline inspect/search | Not started | Read-only queries, FTS5, stable ties and versioned JSON |
| P2.1–P2.4 — GitHub acquisition/recovery | Not started | Typed transport, complete pagination, lease, checkpoints, isolated failures |
| P3.1–P3.4 — Reviews and health | Not started | PR base/head + reviews, review threads, coverage and explicit retry |
| P4.1–P4.5 — Retrieval and analysis | Not started | Versioned documents, embeddings, semantic search, clustering, refresh |
| P5.1–P5.2 — TUI | Not started | Responsive shared-engine browser and maintainer actions |
| P6.1 — V2 scope and packaging | Not started | Release only selected local workflows; deferred scope absent |

A task is complete only when its acceptance checks pass. Keep deferred capabilities absent from code,
workspace members, runtime dependencies, command help, and schema.
