# Gitcrawl compatibility ledger

## Baseline

This ledger records behavior from the read-only Gitcrawl reference checkout at
`/Users/joshka/local/gitcrawl/default`, change
`ymmxytsluuktnvuwqqmrrsoqqmtyvpls`, commit
`8c9a4f85b7c4eaae5b7d279c2e83c2eb167bed3a`. The archive schema at that revision is version 13.
The source checkout was not modified.

The inventory uses `README.md`, `SPEC.md`, `docs/commands.md`, the feature documentation, the
current CLI dispatch, store schema and behavior tests, and CrawlKit v0.16.5. CrawlKit's remote
contract, auth, archive query/publication, config, SQLite, snapshot, progress, and vector packages
were inspected as interfaces, not adopted as Rust dependencies.

Statuses describe the Forgesync plan:

- **Retained**: preserve the useful user-visible behavior, with the Rust interface in this plan.
- **Different**: preserve the outcome with an intentional interface, schema, or ownership change.
- **Deferred**: planned for a later phase; not part of earlier milestone acceptance.
- **Unsupported**: not planned for the initial product; do not imply compatibility.

The compatibility target is behavior and evidence, not Go package structure, SQL layout, every flag,
or OpenClaw-specific integration. Forgesync is a local GitHub archive and does not require
OpenClaw, Octopool, a server, or a model provider for local reads.

## Command inventory

References below are relative to the Gitcrawl checkout unless a CrawlKit path is named.

### Setup, configuration, and diagnostics

| Gitcrawl command or surface | Status | Forgesync destination | Evidence |
| --- | --- | --- | --- |
| `init` | Different | `forgesync init --archive`; P1.2 | `internal/cli/init.go`, `docs/configuration.md` |
| `doctor` | Retained | `forgesync doctor --json`; P3.4 | `internal/cli/doctor.go`, `docs/commands.md` |
| `status` | Retained | `forgesync status`; P1.4, then P3.4 | `internal/cli/control.go`, `internal/store/store.go` |
| `version`, global `--version` | Retained | Binary version and Clap help; P0.2 | `internal/cli/app.go`, `internal/cli/help.go` |
| `metadata` | Unsupported | No CrawlKit control manifest in the local CLI | `internal/cli/control.go`, CrawlKit `control/` |
| `configure` | Different | TOML config resolution; no config-edit wizard initially | `internal/config/config.go`, `docs/configuration.md` |
| `check-update` | Unsupported | Release update checks are outside archive workflows | `internal/cli/releasecheck.go` |
| `remote login`, `whoami`, `archives`, `status` | Deferred | Explicit remote client only after P6.3 contract review | `internal/cli/remote_commands.go`, CrawlKit `remote/` |
| `cloud publish` | Deferred | Explicit remote publication only after P6.3 contract review | `internal/cli/cloud_commands.go`, `internal/cli/cloud_contract.go` |
| `serve` | Unsupported | No local HTTP server is in scope | `SPEC.md`, `internal/cli/app.go` |
| global JSON output | Different | Versioned `{schema_version, command, data, warnings, error}` envelope; P0.2 onward | `internal/cli/output.go`, `docs/automation.md` |
| config/env resolution | Different | CLI > documented env > TOML > defaults; token values stay in referenced env vars | `internal/config/config.go`, `docs/configuration.md` |

### Acquisition and archive operations

| Gitcrawl command | Status | Forgesync destination | Evidence |
| --- | --- | --- | --- |
| `sync` | Retained | `sync`; metadata, closed sweep, explicit scopes, and child families; P2.1–P3.2 | `internal/syncer/`, `docs/sync.md` |
| `sync-failures` | Retained | Failure and retry records; P2.4 | `internal/cli/inspect.go`, `internal/syncer/failure_isolation_test.go` |
| `coverage` | Retained | Family-aware status and freshness; P1.3–P3.4 | `internal/store/archive_coverage.go`, `docs/commands.md` |
| `fill-pr-details` | Different | PR detail family within `sync --with pr-details`; P3.1 | `internal/cli/sync.go`, `internal/syncer/pull_details.go` |
| `capture` | Retained | Versioned code-free conversation JSON export; P3.4 | `internal/capture/`, `docs/capture.md` |
| `refresh` | Retained | Compose existing sync/enrich/embed/cluster operations; P4.5 | `internal/cli/refresh.go`, `docs/refresh-and-embed.md` |
| `runs` | Retained | Durable operation history; P2.3 onward | `internal/store/runs.go`, `docs/commands.md` |
| `portable refresh` | Deferred | Explicit immutable Git snapshot fetch/install; P6.2 | `internal/cli/portable_refresh.go`, `docs/portable-stores.md` |
| `portable export` | Different | Local `snapshot export`; P6.1 | `internal/portable/`, `docs/portable-stores.md` |
| `portable prune` | Different | Export a staged consistent snapshot; never mutate the active archive; P6.1 | `internal/cli/portable_commands.go`, `internal/store/portable_schema.go` |
| legacy `export-sync`, `import-sync`, `validate-sync` | Unsupported | Use the explicit Gitcrawl archive importer and snapshot format instead | `internal/cli/app.go`, `docs/commands.md` |
| legacy `portable-size`, `sync-status`, `optimize` | Unsupported | No separate compatibility commands planned | `internal/cli/app.go`, `docs/commands.md` |

Sync must retain committed successes when other resources fail, distinguish incomplete from complete
empty collections, resolve a family failure only after that family commits, and resume without
advancing a checkpoint past unobserved content. Default scope is open content plus a bounded closed
sweep; `all` is historical backfill. Explicit number/date scopes cannot advance unrelated repository
watermarks. See P2.3–P2.4 and P3.1–P3.2 acceptance gates.

### Queries, analysis, and code

| Gitcrawl command | Status | Forgesync destination | Evidence |
| --- | --- | --- | --- |
| `threads` | Retained | `inspect` and local thread listing; P1.4 | `internal/cli/inspect.go`, `internal/store/threads.go` |
| direct `search` | Retained | Cross-repository keyword/semantic/hybrid query; P1.4 and P4.3 | `internal/cli/search.go`, `internal/store/search.go` |
| `search issues` or `search prs` (`gh search` shape) | Different | Local cached query compatibility only if justified by fixtures; not live GitHub search | `internal/cli/gh_search.go`, `docs/search.md` |
| `neighbors` | Retained | Similar-thread query; P4.3–P4.4 | `internal/cli/neighbors.go`, `docs/clustering.md` |
| `summarize` | Retained | Versioned summary service; P4.5 | `internal/cli/summarize.go`, `internal/openai/summaries.go` |
| `embed` | Retained | Optional compatible embedding service; P4.1–P4.3 | `internal/cli/embed.go`, `internal/openai/client.go` |
| `cluster` | Retained | Deterministic graph building and durable results; P4.4 | `internal/cluster/build.go`, `docs/clustering.md` |
| `clusters`, `clusters-report`, `durable-clusters`, `cluster-detail`, `cluster-explain` | Retained | Cluster queries/reports; P4.4 | `internal/cli/clusters.go`, `docs/clustering.md` |
| `code index` | Deferred | Independent ignored-file-aware source index; P7.1 | `internal/codeindex/`, `docs/code-index.md` |
| `key-summaries` | Deferred | Summary capability only if its distinct behavior is confirmed; P4.5 | `internal/cli/app.go`, `internal/store/summary_tasks.go` |
| `cluster-experiment` | Unsupported | Experimental surface, no launch contract | `internal/cli/app.go`, `internal/cli/cluster_graph.go` |
| `merge-clusters`, `split-cluster` | Unsupported | No merge/split command in the planned governance contract | `internal/cli/app.go`, `docs/governance.md` |
| `completion` | Unsupported | Shell completion is not an archive behavior | `internal/cli/app.go`, `docs/commands.md` |

Keyword search must work offline and without model credentials. Semantic search only uses compatible
current vectors; unavailable providers are explicit errors unless keyword fallback was requested.
Clustering keeps maintainer decisions separate from regenerated membership and never retires unseen
clusters under incomplete coverage.

### Governance and terminal interface

| Gitcrawl command | Status | Forgesync destination | Evidence |
| --- | --- | --- | --- |
| `close-thread`, `reopen-thread` | Retained | Local-only maintainer decisions; P4.4 | `internal/cli/governance.go`, `docs/governance.md` |
| `close-cluster`, `reopen-cluster` | Retained | Local-only decision history; P4.4 | `internal/cli/governance.go`, `docs/governance.md` |
| `exclude-cluster-member`, `include-cluster-member` | Retained | Local-only member decision; P4.4 | `internal/cli/governance.go`, `docs/governance.md` |
| `set-cluster-canonical` | Retained | Local-only representative choice; P4.4 | `internal/cli/governance.go`, `docs/governance.md` |
| `tui` | Retained | Ratatui browser and actions using engine APIs; P5.1–P5.2 | `internal/cli/tui_*.go`, `docs/tui.md` |
| TUI `--json` snapshot | Different | Standard CLI JSON envelope and shared query output | `internal/cli/tui_command.go` |

All governance actions remain local and do not write to GitHub. The UI is a client of application
services, not a second workflow implementation.

### GitHub and portability compatibility surfaces

| Gitcrawl surface | Status | Forgesync destination | Evidence |
| --- | --- | --- | --- |
| `gh` shim | Unsupported | No OpenClaw or Octopool special case; user may run `gh` separately | `internal/cli/gh_migrated.go`, `docs/gh-shim.md` |
| GitHub REST and GraphQL acquisition | Retained | Typed GitHub provider; P2.1–P3.2 | `internal/github/`, `docs/sync.md` |
| portable manifest and subscriber contract | Deferred | Versioned local format then explicit Git transport; P6.1–P6.2 | `internal/portable/`, `docs/portable-stores.md` |
| CrawlKit remote query/auth contract | Deferred | Adapter only for a verified deployed contract; P6.3 | `internal/cli/remote_commands.go`, CrawlKit `remote/contract.go`, `docs/cloud-archives.md` |
| CrawlKit staged snapshot publish contract | Deferred | Implement only if independent client use is confirmed; P6.3 | `internal/cli/cloud_contract.go`, CrawlKit `remote/` and `docs/remote-contract.md` |

The inspected CrawlKit v0.16.5 interface has bearer-authenticated archive query and publication,
contract discovery, login/token exchange, role and archive status, ingest, and chunked SQLite bundle
upload routes. Gitcrawl requires advertised app capabilities, route auth, reader query arguments,
ingest columns, snapshot provenance/staging/atomicity, and gzip-upload capabilities before publish.
P6.3 must verify the deployed service contract and fixtures; the plan does not authorize building or
deploying a replacement service.

## Persistent data inventory

The source schema is version 13 in `internal/store/schema.go`; migration logic is in
`internal/store/store.go`. These data families are retained or deliberately rebuilt into the new
format. Forgesync never copies Gitcrawl local primary keys as durable provider identities.

| Family | Gitcrawl tables | Forgesync disposition |
| --- | --- | --- |
| Repository and thread identity | `repositories`, `threads` | Retain facts; host-qualified provider identity; P1.1–P2.2 |
| Comments and revisions | `comments`, `comment_revisions`, `thread_revisions` | Retain current and historical evidence; P2.4–P3.2 |
| Raw provenance and content blobs | `blobs`, raw JSON columns | Retain provenance selectively; not workflow API values; P1.3 onward |
| Observation ordering and membership | `thread_observation_sequence`, `thread_child_observation_reservations`, `thread_child_observation_memberships`, `workflow_run_observation_reservations` | Redesign around monotonic scoped observations and family coverage; P1.3 |
| Pull request evidence | `pull_request_details`, `pull_request_files`, `pull_request_commits`, `pull_request_checks`, `github_workflow_runs` | Retain; head/repository scoping and duplicate file paths are mandatory; P3.1 |
| Review discussions | `pull_request_review_threads`, `pull_request_review_thread_revisions`, `pull_request_review_thread_syncs` | Retain resolution, tombstone, revision, and restoration evidence; P3.2 |
| Source snapshots | `thread_code_snapshots`, `thread_changed_files`, `thread_hunk_signatures` | Retain the PR evidence semantics; source index is independent and later; P3.1, P7.1 |
| Search documents | `documents`, `documents_fts`, `code_snapshots`, `code_documents`, `code_documents_fts` | Rebuild from canonical content; keyword indexes are derived; P1.4, P4.1, P7.1 |
| Generated summaries | `document_summaries`, `thread_key_summaries`, `summary_runs` | Retain only with source/document/model provenance; P4.5 |
| Vectors and fingerprints | `document_embeddings`, `thread_vectors`, `thread_fingerprints`, `embedding_runs` | Recompute if compatibility cannot be proven; P4.1–P4.3 |
| Sync and failure history | `sync_runs`, `sync_attempt_failures`, `repo_sync_state` | Retain outcomes, failures, and valid checkpoints; redesign run/job records; P2.3–P2.4 |
| Cluster output | `cluster_runs`, `similarity_edges`, `clusters`, `cluster_members`, `cluster_groups`, `cluster_memberships` | Retain semantics; deterministic graph and stable correspondence; P4.4 |
| Maintainer decisions | `cluster_overrides`, `cluster_events`, `cluster_aliases`, `cluster_closures` | Import and preserve separately from generated state; P3.3–P4.4 |
| Portable-only metadata | `portable_metadata` plus manifest fields | Replace with the versioned Forgesync snapshot manifest; P6.1 |
| Owner exclusions | No baseline table | Planned as new durable policy, not claimed as existing Gitcrawl behavior; P7.2 |

Portable exports intentionally omit or transform data: FTS/documents, vectors, code index, run
history, similarity edges, blobs, raw payloads, and usually sync failures. Exported coverage,
capabilities, body truncation, and excluded tables are declared in metadata and a manifest. Forgesync
snapshot manifests must state repository scope, included families, redaction/truncation, compression,
size, and digest. Import reports all omissions and does not claim uncertain evidence is fresh.

## Baseline invariants and regression entry points

These behaviors are accepted operational requirements in the plan and have existing regression
tests. P0.3 will convert the cases into sanitized named fixtures and a truth table.

| Invariant | Reference evidence | Planned gate |
| --- | --- | --- |
| Delayed observations cannot replace newer accepted evidence | `internal/store/observation_order_test.go`, `internal/store/thread_enrichment_test.go` | P0.3, P1.3 |
| Identical observations are idempotent; tied conflicting observations are rejected | `internal/store/observation_order_test.go` | P0.3, P1.3 |
| Source time, fetch sequence, completeness, and family are independent | `internal/store/archive_source_test.go`, `internal/store/observation_order_test.go` | P0.3, P1.3 |
| Complete empty child collections remove membership; incomplete ones preserve it | `internal/store/observation_order_test.go`, `internal/syncer/partial_sync_test.go` | P0.3, P2.4 |
| Comment success does not make other child families current | `internal/store/observation_order_test.go` | P0.3, P1.3 |
| Failure bookkeeping does not roll back unrelated completed work | `internal/syncer/failure_isolation_test.go` | P0.3, P2.4 |
| Closed sweeps cover offline intervals and advance only after success | `internal/syncer/closed_sweep_test.go` | P0.3, P2.3 |
| Duplicate PR file paths remain valid | `internal/store/pull_requests_test.go` | P0.3, P3.1 |
| Check/workflow evidence is scoped to repository and head | `internal/store/pull_requests_test.go`, `internal/syncer/check_runs_test.go` | P0.3, P3.1 |
| Review thread tombstones, revisions, and restores survive | `internal/store/review_threads_test.go` | P0.3, P3.2 |
| Cluster graph scoring is deterministic | `internal/cli/cluster_graph_scoring_test.go` | P0.3, P4.4 |
| Snapshot failure retains the source | `internal/portable/export_failure_paths_test.go` | P0.3, P6.1 |

The simple comparator tests specify: valid source timestamps outrank malformed/missing ones; among
valid timestamps, later source time wins; equivalent or both absent timestamps fall through to the
observation sequence; equal malformed timestamps fall through to sequence; distinct malformed
timestamps are ambiguous errors. Revision evidence uses positive fetch sequence first, then source
clock when sequence is unavailable. Child reservations advance independently by family. Parent
revision freshness additionally checks the accepted evidence generation and source-clock fence;
the SQL consumers and migration tests remain required P1.3 discovery evidence.

## Proposal boundary

The repository-metrics proposal [#206](https://github.com/openclaw/gitcrawl/pull/206), analytics and
review-state proposal [#216](https://github.com/openclaw/gitcrawl/pull/216), and owner-directed
exclusions proposal [#217](https://github.com/openclaw/gitcrawl/pull/217) were recorded by the plan
as unmerged at investigation time. They are not evidence of baseline behavior. Metrics and analytics
remain outside the initial scope. P7.2 explicitly adds exclusions as a Forgesync product decision;
it is not described as a Gitcrawl compatibility feature. Recheck live status only if later work
depends on one of these proposals.
