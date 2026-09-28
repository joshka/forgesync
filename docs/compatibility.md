# Gitcrawl compatibility ledger

## Baseline

This inventory uses the read-only Gitcrawl checkout at
/Users/joshka/local/gitcrawl/default, change
ymmxytsluuktnvuwqqmrrsoqqmtyvpls, commit
8c9a4f85b7c4eaae5b7d279c2e83c2eb167bed3a. Its archive schema is version 13. The Go checkout and
production archives were not modified.

The reference review covered the repository instructions, README, SPEC, command and feature docs,
CLI dispatch, store schema/migrations, selected sync/GitHub/store/capture/portable code, and
regression tests. CrawlKit v0.16.5 remote contract, auth/query/publication, config, SQLite, snapshot,
progress, and vector interfaces were inspected. They inform behavior only; Forgesync does not depend
on CrawlKit.

This ledger follows the revised Forgesync v2 plan and implementation handoff in
/Users/joshka/local/gitcrawl/.plans. The product is a focused local Rust v2, not full Gitcrawl
parity. Statuses mean:

- **retain**: preserve the selected user outcome and its correctness requirements.
- **redesign**: preserve the user need through the single v2 interface or a new local model.
- **defer**: omit code, dependencies, commands, and tables from v2; keep Gitcrawl usable separately.

## CLI migration table

Evidence paths are relative to the reference checkout. The proposed v2 command tree is in the
selected plan's CLI review section. Commands appear only as implementation phases make them usable.

| Gitcrawl surface | Status | Forgesync v2 destination and reason | Reference evidence |
| --- | --- | --- | --- |
| init | redesign | archive init --archive creates only a new archive | internal/cli/init.go; docs/configuration.md |
| migrate | redesign | archive migrate is explicit; open and read never migrate | internal/store/store.go |
| status, doctor | redesign | archive status and doctor report family coverage, schema history, lease ownership, and durable work | internal/cli/control.go; internal/cli/doctor.go |
| version command | redesign | Use Clap --version only | internal/cli/app.go |
| metadata, check-update | defer | Control manifests and update checks do not support local acquisition or retrieval | internal/cli/control.go; internal/cli/releasecheck.go |
| configure | redesign | CLI options resolve over documented env, TOML, and defaults; no edit wizard | internal/config/config.go; docs/configuration.md |
| sync | redesign | One typed sync request with repositories or --all, state, and selected --with comments,reviews,review-threads families | internal/cli/sync.go; docs/sync.md |
| sync-failures, coverage | redesign | archive status and run list/show expose durable failures and per-family coverage | internal/cli/inspect.go; internal/store/archive_coverage.go |
| fill-pr-details | defer | Files, commits, checks, and workflow runs are outside selected v2 scope | internal/cli/sync.go; internal/syncer/pull_details.go |
| capture | defer | Separate conversation export is not needed for the selected archive workflow | internal/cli/capture.go; docs/capture.md |
| refresh | redesign | Compose sync and explicitly selected analysis via --analyze; --plan performs no network work | internal/cli/refresh.go |
| runs | redesign | run list/show inspect durable work; run retry optionally filters unresolved work with --family | internal/store/runs.go |
| code index | defer | Source indexing is independent of discussion acquisition | internal/codeindex/; docs/code-index.md |
| threads | redesign | thread list and thread show share one checked reference model | internal/cli/inspect.go |
| direct search and issues/prs search shape | redesign | One search QUERY with typed repeatable repo/state/kind/mode filters; no hidden qualifier grammar | internal/cli/search.go; internal/cli/gh_search.go |
| neighbors | redesign | thread related uses the shared retrieval and ranking policy | internal/cli/neighbors.go |
| summarize, key-summaries | defer | Summary generation is explicitly outside v2 | internal/cli/summarize.go; internal/store/summary_tasks.go |
| embed | retain | Optional embeddings are independent of keyword workflows | internal/cli/embed.go; internal/openai/client.go |
| cluster, clusters, reports, durable views, detail/explain | redesign | `cluster build/list/show/dismiss/restore/exclude/include/canonical` use stable IDs, shared evidence queries, and archive-local decisions | internal/cli/cluster.go; internal/cli/clusters.go; P4.4 |
| close-thread, reopen-thread | redesign | thread dismiss/restore changes local triage state, not GitHub open/closed state | internal/cli/governance.go |
| close-cluster, reopen-cluster | redesign | cluster dismiss/restore persists local decisions across regeneration | internal/cli/governance.go |
| exclude/include cluster member, set canonical | redesign | cluster exclude/include/canonical retain selected local maintainer choices | internal/cli/governance.go |
| tui and tui --json snapshot | redesign | tui launches only the interactive client; ordinary read commands provide data | internal/cli/tui_command.go; docs/tui.md |
| portable refresh/export/prune | defer | No snapshot or Git transport dependency in v2 | internal/cli/portable_*.go; internal/portable/ |
| remote login/status/archives/whoami and cloud publish | defer | No hosted archive, remote auth, or publication dependency in v2 | internal/cli/remote_commands.go; internal/cli/cloud_*.go; CrawlKit remote/ |
| gh shim | defer | Forgesync has no OpenClaw or Octopool special case and performs no GitHub writes | internal/cli/gh_migrated.go; docs/gh-shim.md |
| serve | defer | A local HTTP service is outside the local CLI/TUI product | SPEC.md |
| merge/split clusters and other reserved commands | defer | No stubs or commands without selected behavior | internal/cli/app.go; docs/commands.md |
| global JSON and result output | redesign | One versioned JSON envelope; stdout is results and stderr/file is diagnostics | internal/cli/output.go; docs/automation.md |
| legacy argv reordering, gh-search parser, overloaded JSON, no-op flags | redesign | Standard Clap parsing; one search grammar and one boolean --json contract | internal/cli/args.go; internal/cli/gh_search.go |

## Selected feature disposition

| Feature or data family | Status | Required v2 behavior | Reference evidence and fixture gate |
| --- | --- | --- | --- |
| Multi-repository GitHub identity and metadata | retain | Host-qualified stable provider IDs; mutable owner/name is display and lookup data | internal/store/schema.go; internal/github/provider_ids_test.go; P0.3/P2.2 |
| Issues, pull requests, and source state | retain | Normalize provider data; preserve unknown values; source open/closed stays separate from local triage | internal/store/schema.go; internal/store/threads_test.go; P0.3/P1.1 |
| Comments and current membership | retain | Stage paginated results; only a complete response replaces membership; distinguish complete empty from missing | internal/store/comments_test.go; internal/syncer/partial_sync_test.go; P0.3/P2.4 |
| PR base/head metadata and reviews | retain | Acquire independently with source/head context and per-family coverage | internal/syncer/pr_metadata_test.go; P3.1 |
| Review threads and resolution | retain | Typed GraphQL pagination; inspect partial errors; incomplete results cannot tombstone membership | internal/store/review_threads_test.go; P3.2 |
| Explicit thread references | retain | OWNER/REPO#NUMBER or GitHub URL; bare numbers need repository context; reject conflicting scope | internal/cli/references.go; P0.2/P1.1 |
| Observation ordering and completeness | retain | Sequence, source time, family, request scope, and pagination completeness remain distinct | internal/store/observation_order_test.go; P0.3/P1.3 |
| Runs, leases, checkpoints, retryable failures | retain | Recover committed work, fence stale writers, and retry only recorded failed/deferred families | internal/store/runs_test.go; internal/syncer/failure_isolation_test.go; P2.3/P2.4/P3.4 |
| Offline inspect and FTS5 keyword search | retain | Read-only local queries, stable order/pagination, cross-repository filters, honest coverage | internal/store/search_test.go; P1.4 |
| Documents and embeddings | retain | P4.1 adds versioned deterministic inputs and configurable source-evidence recipes; P4.2 adds validated OpenAI-compatible vectors; P4.3 adds paged exact cosine and RRF with explicit keyword fallback; keyword use needs no model | internal/documents/; internal/vector/exact_test.go; P4.1–P4.3 |
| Related threads and clustering | retain | Bounded deterministic graph; stable IDs; incomplete coverage cannot retire unseen clusters | internal/cluster/build_test.go; internal/cli/cluster_graph_scoring_test.go; P4.4 |
| Local maintainer decisions | retain | Canonical, excluded-member, and dismissed state survives generated updates; no GitHub writes | internal/store/clusters_test.go; P4.4/P5.2 |
| Ratatui browser and actions | retain | Background engine queries, bounded messages, stale-result suppression, reliable terminal restore | internal/cli/tui_*.go; P5.1/P5.2 |
| Repository metrics and analytics | defer | No historical metrics schema or dashboard without a later scope decision | proposal PRs 206/216; not baseline requirements |
| Owner-directed erasure | defer | Cluster member exclusion is selected; archive-wide purge and snapshot policy are not | proposal PR 217; no baseline table |
| PR files, commits, checks, workflow runs | defer | No deep PR-detail tables or API requests in v2 | internal/store/pull_requests_test.go; selected plan P3.1 |
| Full content/review revision history and raw blob archive | defer | Keep enough provenance for current selected evidence; do not copy event/revision machinery | internal/store/thread_enrichment_test.go; internal/store/review_threads_test.go |
| Summaries and generated key summaries | defer | No summary model endpoint, prompt, tables, or CLI command | internal/openai/summaries.go; internal/store/summary_tasks.go |
| Source code indexing and code search | defer | No filesystem walker, source tables, or mixed code/discussion search | internal/codeindex/; docs/code-index.md |
| Gitcrawl database import and old schema migration | defer | Fresh sync is adoption path; old archive remains readable by Gitcrawl | internal/store/store.go; plan P3.3 |
| Portable snapshots, Git distribution, and CrawlKit cloud | defer | No snapshot, Git transport, remote client, login, or cloud dependency | internal/portable/; internal/cli/cloud_*.go; CrawlKit remote/ |
| GitHub write-back, generalized providers, OpenClaw integration | defer | GitHub is read-only acquisition; one explicit provider; local use needs no hosted service | SPEC.md; internal/github/ |

## Persistent table inventory

Gitcrawl's current schema is in internal/store/schema.go; migrations are in
internal/store/store.go. This inventory accounts for every declared table family in that schema.
Forgesync uses a new format and does not copy Gitcrawl primary keys or require legacy tables.

| Gitcrawl tables | V2 disposition |
| --- | --- |
| repositories, threads | Retain concepts; redesign identities and typed domain values |
| comments | Retain selected current discussion content |
| pull_request_details, pull_request_files, pull_request_commits, pull_request_checks, github_workflow_runs | Defer deep PR detail families |
| pull_request_review_threads | Retain current review-thread membership and resolution |
| thread_observation_sequence, thread_child_observation_reservations, thread_child_observation_memberships, workflow_run_observation_reservations | Redesign selected observation ordering and per-family coverage; no workflow-run table |
| thread_revisions, comment_revisions, pull_request_review_thread_revisions, pull_request_review_thread_syncs, blobs | Defer full revision/tombstone/raw-payload history |
| thread_code_snapshots, thread_changed_files, thread_hunk_signatures, code_snapshots, code_documents, code_documents_fts | Defer source indexing and code context |
| documents, documents_fts | Retain deterministic discussion documents and FTS5 as derived data |
| document_embeddings, thread_vectors | Retain compatible optional embeddings; use a deliberate v2 representation |
| document_summaries, thread_key_summaries | Defer summaries |
| thread_fingerprints | Defer unless a selected cluster invariant demonstrates a direct need |
| sync_runs, sync_attempt_failures, repo_sync_state | Redesign as runs, jobs, failures, checkpoints, and truthful coverage |
| summary_runs, embedding_runs | Defer summary history; retain necessary operation/run provenance for embeddings |
| cluster_runs, similarity_edges, clusters, cluster_members, cluster_groups, cluster_memberships | Retain cluster outputs with stable public IDs and deterministic membership |
| cluster_overrides, cluster_events, cluster_aliases, cluster_closures | Redesign only the selected canonical, local dismiss/restore, and member exclusion decisions |
| portable_metadata | Defer with all portable/snapshot distribution |

Coverage and provenance describe absent, incomplete, failed, and successfully empty data separately.
They are not inferred from row counts. Any source/head context that affects review interpretation is
stored with the selected evidence. Local dismissal is not provider state. Local member exclusion is
not owner-directed content erasure.

## Selected regression matrix

These tests are the entry points for selected behavior, not a requirement to translate Go test
syntax. P0.3 turns sanitized scenarios into named Rust fixtures.

| Invariant | Reference tests | Rust gate |
| --- | --- | --- |
| Delayed observations cannot replace newer accepted evidence | internal/store/observation_order_test.go | P0.3, P1.3 |
| Failure records do not roll back unrelated successful acquisition | internal/syncer/failure_isolation_test.go | P0.3, P2.4 |
| Closed sweep covers offline intervals | internal/syncer/closed_sweep_test.go | P0.3, P2.3 |
| Partial sync retains acquired work | internal/syncer/partial_sync_test.go | P0.3, P2.4 |
| Unchanged comments can be reused safely | internal/syncer/comment_reuse_test.go | P0.3, P2.4 |
| Current review membership and restoration remain truthful | internal/store/review_threads_test.go | P0.3, P3.2 |
| Cluster graph scoring remains deterministic | internal/cli/cluster_graph_scoring_test.go | P0.3, P4.4 |

The observation comparator and revision consumers need more than a top-level timestamp rule. The
P0.3 fixture catalog records the selected truth table, including sequence fallback, equivalent and
malformed clocks, incomplete generations, child-family reservations and freshness, parent freshness,
and atomic rollback. No contradictory example was established in the initial bounded inspection.

## Proposal boundary

Repository metrics (#206), analytics and review state (#216), and owner-directed exclusions (#217)
were proposals, recorded as unmerged in the design investigation. They are not baseline behavior.
The revised v2 scope defers all three. Recheck proposal status only if later work explicitly depends
on one.
