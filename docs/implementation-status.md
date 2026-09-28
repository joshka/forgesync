# Implementation status

## Current status

- Current next task: **P0.2 — Bootstrap the Rust workspace**.
- Completed: **P0.1 — Capture the baseline**.
- Reference checkout: `/Users/joshka/local/gitcrawl/default`.
- Reference change: `ymmxytsluuktnvuwqqmrrsoqqmtyvpls`.
- Reference commit: `8c9a4f85b7c4eaae5b7d279c2e83c2eb167bed3a`.
- Reference archive schema: version 13.
- Reference source and archive data were not modified.

## P0.1 evidence

Read the reference `AGENTS`/skill guidance, `README.md`, `SPEC.md`, command/configuration, sync,
search, clustering, governance, capture, TUI, portable-store, and cloud documentation; inspected CLI
dispatch, schema/migrations, relevant GitHub/sync/store/capture/portable implementations, and the
listed regression tests. Inspected CrawlKit v0.16.5 remote contract, auth/query/publication,
configuration, SQLite, snapshot, progress, and vector API packages.

`docs/compatibility.md` records every documented or dispatched Gitcrawl command surface, all current
persistent table families, explicit differences, later planned work, non-goals, and regression
entry points. It separates implemented behavior from commands that currently return
"not implemented" and from proposals the implementation plan says were unmerged at inventory time.

The observation order test cases agree with the minimum plan invariants. The current implementation
has additional revision-consumer, migration, parent-generation, and workflow-head behavior; P0.3
must fixture those paths before P1.3 chooses Rust storage or comparator details. No conflicting
examples were established from this bounded inventory.

Validation: `markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml
docs/compatibility.md docs/implementation-status.md` passed with 0 issues.

## Task sequence

| Task | Status | Evidence or next gate |
| --- | --- | --- |
| P0.1 — Capture the baseline | Complete | `docs/compatibility.md`; reference revision pinned above |
| P0.2 — Bootstrap the workspace | Next | Core/store/CLI crates, stable pinned toolchain, lockfile, CI, instructions, CLI smoke |
| P0.3 — Build the fixture catalog | Not started | Sanitized named regression cases and complete observation ordering truth table |
| P1.1–P7.3 | Not started | Follow dependencies and acceptance gates in the implementation plan |
