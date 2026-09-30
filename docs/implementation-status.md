# Implementation status

## Current position

- Embedding setup failures now implement the standard error traits and retain their typed cause;
  configuration/client phase, CLI codes, and displayed messages remain separate concerns.
- Follow-up cleanup gives six CLI workflows a shared interruption lifetime owner. The listener stops
  on scope exit while engine cancellation retains durable cleanup and reports. CLI tests and Clippy
  passed. The Rust conventions now record this task-lifetime rule.
- The function/state ownership pass covers all twelve surveyed areas. The maintainability plan now
  records the implemented owners, focused evidence, and reasons for retaining linear SQL and simple
  display mappings. Reusable rules cover shared eligibility policy, transaction ownership, and
  cursor advancement after post-query filtering. Workspace tests, Clippy, strict Rustdoc including
  private items, nightly formatting, the CLI build without default features, rumdl, and Markdownlint
  passed.
- Semantic pages now separate bound candidate/vector queries, typed candidate rows, evidence
  hydration, and document-level chunk validation. Raw candidate order owns cursor advancement even
  for rejected vectors. Store Clippy and the embedding retry regression passed.
- CLI retry reuses shared provider setup while preserving cancellation JSON and exit status. Embed
  and refresh execution belongs to their parsed command types; argument overrides and refresh
  selection validation now have named methods. CLI Clippy passed.
- CLI and TUI discussion detail now assemble named source, coverage, metadata/body, and timeline
  sections. TUI loading/error resolution precedes its prepared detail view, whose lines still
  determine scroll limits. All 20 TUI state, snapshot, and resize tests passed.
- Repository scans use a terminal outcome enum and expose start, parent application, and cursor
  recording as durable phases beside their traversal. Both enumeration regressions pass, including
  page-two failure and replay.
- Provider requests now own immutable attempt data and separate budgeted attempts, retry waits,
  redirect traversal, sending, and bounded response decoding. Credential origin checks remain before
  request construction; transport regressions cover cancellation, retry, and redirects.
- Cluster candidates now separate evidence, explicit references, and component policy. One score
  predicate serves neighbor selection and final edges; reference context replaces eight helper
  arguments and a boolean location flag. Cluster workflow and focused unit regressions passed.
- Refresh stages share a validated execution owner and keep acquisition, embeddings, and clusters in
  separate methods. Engine Clippy passed; the focused refresh regression preserves completed
  acquisition when derived analysis cannot run.
- Thread reads now give `ThreadQuery` ownership of bound SQL and deterministic sorting. Stored row
  decoding and page coverage assembly have separate named phases; pagination behavior remains
  covered by the inspect/search regressions.
- TUI action dispatch now delegates to an execution owner with shared cancellation and services.
  Scheduling and progress forwarding remain separate; local decisions have named methods rather than
  boolean behavior parameters. All 20 TUI state and rendering cases passed.
- Canonical storage now separates parent selection from payload/evidence application and child
  reservation checks from complete membership replacement. All seven observation ordering, replay,
  completeness, and rollback regressions passed, together with store Clippy.
- Sync job ownership now distinguishes repository lookup, parent-thread scan jobs, comment jobs, and
  selected pull-request family jobs. Comments and metadata have reserved acquisition owners; job IDs
  and progress travel together. The 14 sync workflow regressions and engine Clippy passed.
- Review and review-thread sync now share a reserved, head-aware collection that owns staging
  progress and consuming terminal writes. Provider collectors keep their page-link and cursor
  behavior. A cross-crate function-shape survey and the next ownership slices are recorded in the
  maintainability plan; Rust conventions now use screen size and parameter count as inspection
  signals for hidden concepts and state machines. All 14 focused sync workflow cases and the
  workspace test suite passed, along with Clippy, strict Rustdoc including private items, nightly
  formatting, the CLI build without default features, and Markdown checks.
- Every Rust module file now opens with a purpose and relationship map, including private workflow
  leaves and focused test modules. The module docs explain archive ordering, child-family
  completeness, engine stage boundaries, command ownership, and TUI state flow at their owning
  layers. `docs/documentation.md` records the rule for future modules and item documentation.
  Workspace Rustdoc with private items and denied warnings, Clippy, nightly formatting, rumdl, and
  Markdownlint passed for this documentation pass.
- The maintainability migration has split crate roots and the largest workflow modules into named
  concepts. Review-thread normalization, core identities, embedding client concerns, and TUI queries
  have smaller owners. Store and CLI integration suites are grouped by scenario, CLI arguments by
  command, and TUI keys by screen. See the [maintainability plan](maintainability-plan.md).
- CLI search, sync, refresh, embed, and cluster commands now pass their parsed arguments directly to
  the owning command module. Cluster build, read, and local decisions have separate files with
  explicit dependencies. The workspace format, Clippy, test, optional CLI build, and documentation
  gates passed after these changes.
- All Rust `use super::*` imports were replaced with explicit dependencies. CLI command and report
  modules now import their types and functions at the point of use, leaving the crate root focused
  on startup and output. Other module glob imports were replaced with named imports. Workspace
  formatting and Clippy passed after the import cleanup.
- Restricted visibility within private CLI and TUI modules and private engine, GitHub, and store
  leaf modules now uses `pub` with the enclosing module as the boundary. Restrictions on public
  types and public modules remain where widening them would expose implementation details.
- `AGENTS.md` now requires reusable maintainer feedback and recurring review findings to be recorded
  in the linked project guidance during the change.
- Source Rustdoc now explains selected helper contracts across core, store, GitHub, engine, CLI, and
  TUI. The comments preserve intent behind content hashes, coverage and ordering decisions,
  pagination and retries, partial outcomes, and UI generation tracking.
- `scripts/README.md` now documents the native binary smoke and packaging scripts, including their
  required inputs, local effects, outputs, and the release workflow that calls them.
- The dependency resolution audit found no compatible package updates with Rust 1.98.1; a newer
  `crypto-common` release remains outside the current compatible resolution.
- P6.1 implementation is in place; hosted Windows, Linux, and Intel macOS platform results remain to
  be collected by CI.
- Next action: **Run the hosted platform matrix before preparing a release**.
- Future maintainability reviews should follow the completed ownership decisions and retained
  linear-code rationale in the plan; apply those inspection signals when adding new behavior.
- Complete: **P0.1 — Capture the baseline and reconcile selected v2 scope**.
- Complete: **P0.2 — Bootstrap the Rust workspace**.
- Complete: **P0.3 — Build the fixture catalog**.
- Complete: **P1.1 — Core identities and outcomes**.
- Complete: **P1.2 — Explicit SQLite lifecycle**.
- Complete: **P1.3 — Observation transactions**.
- Complete: **P1.4 — Offline inspect and search**.
- Complete: **P2.1 — HTTP transport and credentials**.
- Complete: **P2.2 — Thread enumeration**.
- Complete: **P2.3 — Runs, leases, and basic sync**.
- Complete: **P2.4 — Comments and independent failures**.
- Complete: **P3.1 — PR metadata and reviews**.
- Complete: **P3.2 — Review threads**.
- Complete: **P3.4 — Health and explicit retry**.
- Complete: **P4.1 — Versioned documents**.
- Complete: **P4.2 — Embeddings**.
- Complete: **P5.1 — Read-only browser**.
- Complete: **P5.2 — Maintainer actions and live progress**.
- Reference checkout: /Users/joshka/local/gitcrawl/default.
- Reference change: ymmxytsluuktnvuwqqmrrsoqqmtyvpls.
- Reference commit: 8c9a4f85b7c4eaae5b7d279c2e83c2eb167bed3a.
- Reference archive schema: version 13.
- Forgesync repository: /Users/joshka/local/forgesync, initialized as non-colocated jj.
- The Go checkout and production archives were not modified.

## Maintainability migration evidence

The crate roots now expose named concept modules. CLI command families, engine sync, refresh,
search, and clustering, GitHub resource and transport code, store operations, and TUI state and
rendering are grouped by behavior. Large unit suites were moved beside their owners. The
review-thread suite, its normalization, TUI query operations, core identities, and embedding client
concerns received the same treatment in the latest slices. TUI read dispatch names each query
handler, and key routing delegates to browser and triage screen modules. Store observation and read
integration suites and CLI contract suites are grouped by scenario; CLI arguments are grouped by
command. The architecture map and Rust conventions describe the resulting navigation paths.

The CLI dispatcher now constructs command requests and delegates execution to the corresponding
search, sync, refresh, embedding, cluster, or TUI module. Provider setup, cancellation, and archive
opening remain with the command that needs them.

CLI rendering now carries a named output mode after parsing `--json`. Terminal edge navigation uses
a named direction. Repeated host, parser, embedding-response, and TUI rendering cases use named
`rstest` cases so a failure identifies its scenario without a loop in the test body.

Validation of the combined migration on this checkout:

- `rumdl check .` and Markdownlint CLI with the global config: passed.
- `cargo +nightly fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.

The remaining 350–500 line production files were reviewed for ownership: cluster generation,
candidate graph construction, embedding batching, and sync coordination each follow one workflow or
algorithm. File length alone does not justify another module boundary. Revisit a file when a
specific behavior becomes hard to locate or change.

## P0.1 evidence

Read the revised plan's Selected v2 scope, crate/module map, contracts, CLI audit and proposed
command tree, staged tasks, regression matrix, and execution guidance. Read the implementation
handoff and reference checkout instructions. Inspected Gitcrawl's README/SPEC,
command/config/sync/search/ clustering/governance/review-thread/TUI docs, CLI dispatch, schema and
migrations, selected source implementations/tests, and CrawlKit v0.16.5
remote/config/store/snapshot/progress/vector interfaces.

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

Cargo resolved Clap 4.6.7, Serde 1.0.229, serde_json 1.0.151, and assert_cmd 2.2.2. These are
lockfile versions, not raised minimum requirements.

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
fixtures cover interrupted acquisition and review-thread visibility, tombstone, and restore
behavior. No real private discussion content or credentials are included.

`fixtures/scenarios/observation_ordering.json` is the named truth table. It records canonical source
clock ordering, revision acquisition ordering, missing and malformed clocks, incomplete generation
high-water marks and later hydration, family-independent reservations, parent/child freshness,
idempotent replay, conflict rejection, complete-empty versus incomplete membership, and atomic
review refresh. Each selected regression invariant in the compatibility matrix maps to at least one
named scenario. Changed-file fixtures and duplicate paths remain absent because deep PR file data is
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
content; review evidence carries commit/head context. Unknown provider fields use a sorted JSON
object that preserves nested values. Family coverage distinguishes missing, incomplete, complete
(including empty), unavailable, failed, and deferred. Database-independent observations carry
family, payload, source clock, acquisition time, sequence, and collection completeness. Operation
outcomes have distinct complete, partial, deferred, failed, and interrupted states.

The versioned CLI JSON envelope remains in `forgesync-cli`; core types do not depend on the CLI
crate or its output DTOs. Tests cover rejected identity/time inputs, timezone normalization,
archive-time precision, unknown provider-field round trips, and serialized coverage and outcome
states.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 20 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.
- `markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml docs/compatibility.md docs/implementation-status.md`:
  passed, 0 issues.

## P1.2 evidence

Added the SQLite-only SQLx store with an embedded initial migration, Forgesync format identity, UUID
archive identity, and UTC creation time. Creation uses exclusive file creation and refuses to
overwrite an existing path. Read-only and read-write opens require an existing regular file and
reject unsupported, unmigrated, or newer schemas without applying migrations. Migration is a
separate operation. Read connections are read-only; writable handles use a single writer connection,
WAL, FULL synchronous mode, foreign keys, and a five-second busy timeout.

Added `archive init`, `archive migrate`, `archive status`, and `archive doctor`. Status opens the
archive read-only. Doctor runs SQLite quick-check and probes FTS5 and actual foreign-key enforcement
with temporary objects. All four commands have human output and the shared versioned JSON envelope;
errors have stable machine-readable codes.

On-disk tests cover exclusive creation, read-only and read-write open, current-schema migration,
missing paths, refusal of newer, dirty, and checksum-mismatched migration histories, temporary
health probes, FTS5, foreign keys, and CLI JSON results.

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
remain text. Explicit advanced syntax reports malformed FTS as a query error. The CLI adds `search`,
`thread list`, and `thread show`; JSON and human output share the same read-only engine operations.

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
- `markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml docs/compatibility.md docs/implementation-status.md`:
  passed, 0 issues.

## P2.1 evidence

Added the `forgesync-github` crate with a shared HTTPS-first Reqwest client, a four-request
concurrency limit, 30-second request timeout, five-attempt limit, and bounded total retry budget.
Requests use a redacted token type, bound JSON response bodies, and emit tracing spans with only the
configured origin, method, attempt, and status. HTTP errors are typed without retaining raw provider
payloads.

The client attaches bearer credentials only to its configured origin. It validates absolute and
relative pagination URLs, disables automatic redirects, and explicitly follows same-origin redirects
after validating each destination. Cross-origin redirects are rejected before contact. It retries
network failures, 429, selected 5xx responses, and identified 403 rate limits; generic
authentication and permission failures are not retried. Retry and reset hints are honored when they
fit the retry budget; otherwise the operation is deferred.

Added CLI-edge credential discovery in configured environment variable, `GITHUB_TOKEN`, then
host-aware `gh auth token --hostname HOST` order. The `gh` subprocess uses argument arrays,
suppresses stderr, and has timeout and cancellation handling. Local archive commands do not invoke
credential discovery.

Local HTTP tests cover transient server retry, primary rate-limit retry, generic 403 rejection,
retry-budget deferral, cancellation, response and URL origin checks, and token redaction. Credential
tests cover precedence, empty values, invalid environment names, subprocess timeout, and
cancellation without using real credentials.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 60 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.

## P2.2 evidence

Added typed GitHub REST normalization for repository metadata and the combined issues endpoint.
Numeric repository and thread IDs remain stable when a repository is renamed. Issue responses with
the REST `pull_request` object normalize as pull requests; unknown owner, author, label, assignee,
and provider fields remain available in provider data. Pagination follows validated same-origin
`Link` destinations while preserving enterprise API base paths.

Added schema version 4 repository scan records. Each scan stores its sequence, status, start and
update times, committed page and thread counts, safe failure summary, and next page URL. The engine
reserves its sequence before network access, updates the repository by stable provider ID, applies
each thread row independently, and advances the cursor only after every row in that page commits. A
later page failure or cancellation leaves prior rows available and the scan explicitly incomplete.

Local HTTP tests cover repository rename redirects, issue and pull-request normalization, enterprise
pagination, idempotent replay, and a page-two failure that preserves page-one content and its retry
cursor.

Validation:

- `cargo fmt --all`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked --offline`: passed, 66 tests.
- `cargo build -p forgesync-cli --no-default-features --locked --offline`: passed.
- `cargo doc --workspace --no-deps --all-features --locked --offline`: passed.
- Markdown lint: passed, 0 issues.

## P2.3 evidence

Added schema version 5 run, job, failure, archive-lease, and repository-checkpoint records. Archive
leases use monotonically increasing fencing tokens; sync mutations validate the active token in the
same transaction as each write. Heartbeats extend the bounded lease while acquisition runs. A stale
owner cannot write after a later process takes ownership.

Added durable `sync` runs for explicit repository scopes or registered repositories selected with
`--all`. Sync records open and closed thread jobs, applies each committed REST page before advancing
its cursor, and leaves interrupted or failed scans incomplete. The default scope fetches open
threads and performs a closed-thread sweep. Successful closed sweeps advance a watermark; later
sweeps query from a one-day overlap before that watermark. Incomplete sweeps keep the prior
watermark. The engine exposes bounded, non-blocking progress snapshots and final partial,
interrupted, deferred, or complete outcomes. The CLI resolves host-specific credentials, supports
anonymous requests when no credential helper is available, handles Ctrl-C cancellation, and returns
distinct exit codes for partial and interrupted runs.

On-disk acceptance tests cover a second archive handle being denied the lease, stale fencing-token
writes, and a separate CLI process being denied while another process holds the lease. Local HTTP
tests cancel during page two, verify page one remains queryable, replay the run without duplicate
threads, and check that a closed sweep failure preserves its watermark and retries across a
simulated long offline interval with the expected overlap.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked --offline`: passed, 73 tests.
- `cargo build -p forgesync-cli --no-default-features --locked --offline`: passed.
- `cargo doc --workspace --no-deps --all-features --locked --offline`: passed.
- `markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml docs/implementation-status.md`:
  passed, 0 issues.

## P3.1 evidence

Added schema version 7 head context for review snapshots. Pull-request metadata now records
host-qualified base/head repositories and commit IDs, draft and merged state, and the provider's
original branch values. Unknown pull-request and reviewer fields remain available as provenance.
Reviews include normalized state, reviewer provider ID and login, submitted time, reviewed commit,
and the original reviewer object.

Sync refreshes PR metadata as its own family for archived pull requests. `sync --with reviews`
selects paginated review acquisition; ordinary sync makes no review requests. Review membership and
its captured head SHA commit together. A changed head marks the previous review snapshot stale,
while a failed or incomplete refresh keeps both the last complete membership and its head context.
Metadata, review, and comment failures remain isolated. Repositories without pull requests do not
receive empty PR-family jobs.

Local HTTP and archive tests cover enterprise endpoint paths, paginated reviews, branch/reviewer
identity and provenance, changed-head staleness without implicit review fetches, failed-review
preservation of comments and prior reviews, and complete empty review snapshots. CLI parsing covers
`--with reviews`.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked --offline`: passed, 82 tests.
- `cargo build -p forgesync-cli --no-default-features --locked --offline`: passed.
- `cargo doc --workspace --no-deps --all-features --locked --offline`: passed.
- `markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml docs/implementation-status.md`:
  passed, 0 issues.

## P2.4 evidence

Added schema version 6 failure identity and retry fields. Failure records can identify the affected
thread and family, count later retries, record the retrying run, and record successful resolution.
Child-family writes validate the archive lease in the same transaction. The store reports comments
coverage as stale when its parent timestamp or GitHub comment count no longer matches the current
discussion.

Added paginated REST issue-comment acquisition and normalization, preserving unknown provider
fields. `sync --with comments` selects comment acquisition; default sync makes no comment requests.
Each repository/state scope receives a separate comments job. Complete observations atomically
replace membership, including with an empty set. Incomplete observations preserve previous
membership and expose incomplete coverage. Complete snapshots are reused only while the parent
timestamp, expected comment count, and stored membership count all match.

Thread-specific failures are persisted independently so one failed thread does not roll back
successful siblings. Retries skip fresh siblings, update prior retry records, and resolve failures
only after matching complete coverage exists. A failure-ledger write error reports both the original
provider failure and the local write error.

Local HTTP acceptance tests cover page-two failure and selective retry, stale coverage, old
membership preservation, complete-empty versus incomplete-empty collections, independent sibling
success, and original-error retention when the failure ledger rejects a write. CLI parsing covers
the explicit `--with comments` selection.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked --offline`: passed, 79 tests.
- `cargo build -p forgesync-cli --no-default-features --locked --offline`: passed.
- `cargo doc --workspace --no-deps --all-features --locked --offline`: passed.
- `markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml docs/implementation-status.md`:
  passed, 0 issues.

## P3.2 evidence

Added an origin-validated GraphQL POST endpoint derived from the configured REST API base. Typed
review-thread pages preserve GraphQL node IDs separately from numeric REST IDs, page every outer
review-thread connection, and fully acquire each nested comment connection before returning a page.
A page with a nested pagination gap is not representable as a complete page. GraphQL `errors` are
checked even when HTTP succeeds and partial `data` is present; missing or repeated cursors fail the
selected family observation.

Added `sync --with review-threads` as an independently reported pull-request evidence family.
Complete snapshots bind current resolution, outdated state, and nested comments to the currently
observed PR head. Incomplete results retain last complete membership and head provenance. Complete
empty results remove current membership, restoration re-adds it, and a changed head makes prior
review-thread coverage stale. GraphQL family failures do not change comment or review membership.

Local HTTP, sync, and archive tests cover enterprise GraphQL paths, typed outer pagination, nested
comment pagination, partial errors on both connection levels, failed refresh preservation, complete
removal and restoration, head staleness, and transaction rollback of membership, coverage, and head
context. CLI parsing covers `--with review-threads`.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked --offline`: passed, 90 tests.
- `cargo build -p forgesync-cli --no-default-features --locked --offline`: passed.
- `cargo doc --workspace --no-deps --all-features --locked --offline`: passed.
- `markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml docs/compatibility.md docs/implementation-status.md`:
  passed, 0 issues.

## P3.4 evidence

Archive status and doctor now report validated migration history, pending migrations, writer lease
ownership and expiry, family coverage, failed and deferred jobs, in-progress runs, and unresolved
failure counts grouped by evidence family. Diagnostics use read-only queries; doctor retains
temporary-only SQLite integrity and capability probes.

Added `run list`, `run show`, and `run retry`. Retry filters unresolved failure-ledger entries with
repeatable `--family`, groups them by repository and thread scope, and executes each selected scope
through the fenced sync operation. Retry runs record their parent. A previous failure resolves only
after its matching family commits; a comments-only retry leaves review failures unresolved. The
retry plan reports an error when the selected run has no matching unresolved work.

The archive-health fixture covers schema, lease, work counts, and read-only diagnosis. The retry
workflow test covers selected-family filtering, parent linkage, resolution after commit, and
preservation of an unselected review failure.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 93 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.
- `markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml docs/compatibility.md docs/implementation-status.md`:
  passed, 0 issues.

## P4.1 evidence

Added two deterministic document recipes: `original_body` and `discussion_enriched`. The enriched
recipe adds current non-bot comments, submitted reviews, and review-thread state and comments.
Documents carry a recipe version, host-qualified source identity, SHA-256 content hash, normalized
deduplication text, and source update timestamp. Retrieval timestamps are stored separately from the
content hash. The store validates the recipe version and recomputes the hash before persistence;
document writes run under the archive writer fence.

The CLI loads optional TOML configuration from `--config PATH` or `FORGESYNC_CONFIG`. The
`[documents].recipe` setting defaults to `discussion_enriched`; unknown fields and recipe names are
rejected. See [configuration.md](configuration.md).

Tests cover stable hashes for identical normalized evidence, recipe and text invalidation, document
round trips, and persistence behavior across repeated builds, a source timestamp-only change, and an
edited comment.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 102 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.
- Markdownlint on changed documentation: passed.

## P4.2 evidence

Added independent OpenAI-compatible embedding endpoint, model, credential environment variable,
dimension, byte-budget, batch-size, concurrency, timeout, and retry configuration. The reusable
client validates the endpoint, avoids redirects that could forward credentials, bounds response
bodies and total retry time, and validates every returned model, index, vector dimension, finite
value, and nonzero norm. Credentials are read only by the CLI edge and never stored in the archive.

Added deterministic UTF-8 chunking and request batching under per-input and combined-request byte
limits. These are explicit byte budgets and do not infer token limits from provider error prose.
Embeddings are stored as little-endian f32 with model/service identity, dimensions, document hash,
chunk index, and chunk hash. A batch commits independently; retries select only missing chunks whose
document and service identity remain current. Enriched documents exclude stale comments, reviews,
and review-thread evidence.

Local fixtures cover out-of-order and malformed responses, unsafe redirects, byte-bounded chunking
and batches, and a later batch failure followed by a retry that requests only the missing chunk. The
CLI also confirms that an empty repository selection needs no provider request.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 115 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.
- Markdownlint CLI with the global config: passed, 0 issues in the changed documentation.

## P4.3 evidence

Added paged exact semantic retrieval over current vectors matching the configured endpoint, model,
recipe, document hash, source update, and enriched-evidence freshness. Each bounded page is scored
in a blocking worker under a process-wide concurrency limit; cancellation is checked while ranking
and between pages. A discussion with multiple document chunks receives its maximum chunk cosine
score. Hybrid retrieval fuses keyword and semantic ranks with reciprocal rank fusion (constant 60),
and results report the requested/effective mode, ranking, scores, and contributing source ranks.
Missing vectors or provider failures remain explicit unless the caller selects keyword fallback.

The CLI exposes semantic and hybrid modes plus `--keyword-fallback`. Keyword search remains local
and independent of embedding configuration. Local fixtures cover known cosine directions, chunk
maxima, dimension rejection, stable ties, cancellation, RRF scores and provenance, missing model
configuration, and fallback without an extra provider request.

The reproducible exact-cosine benchmark uses the production cosine and ranking implementation with
deterministic 1536-dimensional f32 vectors. Run
`cargo build --release -p forgesync-engine --example exact-cosine-benchmark --locked`, then
`/usr/bin/time -l target/release/examples/exact-cosine-benchmark 10000 1536` and repeat with
`100000`. On an Apple M2 Max with 64 GiB RAM, macOS 26.6.2, and rustc 1.98.1, ranking plus top-20
sorting took 20.192 ms and 204.010 ms, respectively. Peak process RSS was 69,173,248 bytes at 10k
and 636,764,160 bytes at 100k; each vector set contains 61,440,000 and 614,400,000 raw input bytes.
Generation and startup are excluded from the reported ranking time but included in peak RSS. This
measures in-memory cosine ranking and sorting, not SQLite reads or embedding-provider latency.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 120 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.
- Markdownlint CLI with the global config: passed, 0 issues in the changed documentation.

## P4.4 evidence

Added deterministic candidate graph construction from current stored vectors and explicit issue
references. The graph retains Gitcrawl's selected scoring gates: same-kind cosine threshold `0.80`,
cross-kind threshold `0.93`, high-confidence threshold `0.90`, weak title overlap `0.18`, direct
reference score `0.94`, and early body reference evidence within 240 bytes. Per-thread fanout
defaults to 16, connected components are capped at 40 members, and representative selection breaks
degree ties by issue number and stable identity. Graph construction is separate from SQLite
persistence and CLI rendering, runs in a bounded blocking worker, and checks cancellation.

Migration 10 adds cluster runs, stable public cluster IDs, generated memberships, local member
decisions, and decision events. Regeneration matches old and new groups by deterministic member
overlap, keeps the same public ID where groups correspond, and carries canonical, exclusion, and
dismissal decisions forward. Partial vector coverage updates only observed groups and never retires
unseen groups or removes unseen memberships; complete current coverage may retire them.

The engine clusters current open discussions using only complete vectors for the selected endpoint,
model, and document recipe. It holds and heartbeats the archive writer lease across its local vector
snapshot, graph build, and generation write. It does not contact the embedding provider or read an
API key. Reports show eligible and vector counts; zero vectors with eligible discussions return an
actionable unavailable-vector error without writing a run. The CLI implements `cluster build`,
`cluster list`, `cluster show`, `cluster dismiss/restore`, `cluster exclude/include`, and
`cluster canonical` with versioned JSON and local-only decisions.

Reference-scoring unit tests cover similarity safeguards, repository-scoped references, early body
references, bounded fanout, max size, determinism, and cancellation. On-disk store tests cover
stable IDs, partial retention, complete retirement, canonical/exclusion/dismissal persistence, and
input validation. Engine and CLI integration tests cover fresh-vector selection, stale-vector
partial coverage, no-vector rejection, offline build/list, and operation without an embedding API
key.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 128 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.
- Markdownlint CLI with the global config: passed, 0 issues in the changed documentation.

## P4.5 evidence

Added a shared engine refresh operation that composes existing sync, document materialization,
embedding, and cluster operations. `refresh OWNER/REPO` syncs by default; model-backed stages run
only when selected with `--analyze embeddings,clusters`. `--no-sync` supports archive-only analysis.
The standalone `embed` command and refresh embedding stage now share repository paging,
materialization, and embedding policy.

Refresh reports each selected stage's status, partial or complete data, safe failure details, and
the stages that may need another run. Analysis continues after a preceding stage failure, so a
completed sync remains available when an embedding or cluster stage fails. Plain sync does not
construct an embedding client or read its key; cluster-only refresh uses stored vectors and their
configured service identity without contacting the model service.

Engine tests verify sync-only refresh without a model client and preservation of a complete sync
report when the explicitly selected embedding stage is unavailable. CLI tests verify explicit
comma-separated analysis selection and offline cluster refresh without an API key. Reference
behavior was checked against `TestRefreshRunsSyncEmbedAndClusterWithLocalServers`,
`TestClusterAndRefreshStrictVectorsRejectMissingCoverageBeforeMutation`, and
`TestRefreshEmbedsAndClustersWithoutSync` in `internal/cli/app_test.go`, adapted to explicit
analysis selection and independent per-stage reports.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 132 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.
- Markdownlint CLI with the global config: passed, 0 issues in the changed documentation.

## P5.1 evidence

Added the `forgesync-tui` crate and enabled its optional CLI feature in normal builds. The terminal
browser lists registered repositories and discussions, scopes the discussion list to a selected
repository, opens current discussion detail, searches locally by keyword, and shows archive coverage
and recent failed or unfinished runs. Discussion pages contain 100 rows and can be traversed with
`n` and `p`. Narrow terminals stack the three panes; wider terminals place them side by side.

The TUI uses engine read APIs only. Repository, thread, detail, coverage, and run queries execute on
Tokio tasks while the render loop polls input and draws independently. Results use a bounded message
channel, carry a generation number, and stale results are discarded. Exit aborts and joins
outstanding queries; Ratatui terminal handling restores the alternate screen and cursor on normal
return and installs panic restoration. The CLI refuses non-interactive use and `--json` for the TUI.
It does not load app config or embedding credentials for this local-only view;
`--no-default-features` omits the command. See
[tui.md](tui.md).

Behavior was adapted from selected Gitcrawl TUI references, including
`TestTUIUpdateCoversKeyboardStateMachine`, `TestTUIRepositoryPickerSwitchesRepository`, and
`TestTUISyncComponentsClampsDetailViewportAfterResize` in `internal/cli/tui_test.go`. Ratatui
`TestBackend` tests render fixture data at 40×10 and 140×40 and verify detail scrolling clamps after
a resize. Reducer tests cover repository selection, search, responsive keys during pending queries,
and stale-result rejection.

A PTY smoke used a temporary archive with 15 synthetic discussions. It loaded repository and thread
data, opened detail, scrolled through the long body with Page Down, visited coverage and failure
views, accepted a local search, and exited with status 0. The captured terminal sequence restored
the alternate screen and cursor. A CLI integration test confirms non-terminal use returns an
actionable error and ignores irrelevant malformed model configuration.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 141 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed; the TUI dependency is
  optional and the command is omitted.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.
- Markdownlint on changed documentation: passed, 0 issues.

## P5.2 evidence

Added generated-cluster and member browsing to the TUI, including lifecycle, local dismissal, member
inclusion state, canonical role, and generated neighbor scores. Cluster dismissal/restore, member
exclude/include, and canonical selection call the same engine methods used by CLI commands. The
failures view now selects a durable run for explicit retry through `plan_run_retry` and `run_retry`.

The CLI opens the existing archive read-write and resolves GitHub clients at the CLI boundary. `s`
calls `sync_repositories`; `R` calls the shared `refresh` API for full GitHub evidence; `t` retries
the selected run. These actions run in background Tokio tasks. Engine progress remains non-blocking
under a slow terminal, and the TUI reports actual complete, partial, failed, deferred, and
interrupted results. Pressing `q` or `Ctrl+C` during work cancels through a token and waits for the
engine's terminal report, leaving the result visible before exit.

Before remote work starts, the TUI checks read-only archive diagnostics and displays the active
writer owner and expiry. A lease race is surfaced with the latest owner information. Local cluster
decisions use the same fenced engine calls and report the same contention.

The behavior follows Gitcrawl's selected action, local-decision, and progress references, including
`TestTUIRunMenuItemCoversNonExternalActions`, `TestTUIUpdateRefreshMessageBranches`,
`TestTUILocalActionsMutateStore`, `TestTUIClusterMemberOverrideActions`, and
`TestSyncProgressWriterPublishesPrivateSanitizedSnapshot`. TUI reducer tests cover selected
repository scope for sync/refresh, selected cluster member decisions, selected-run retry, stale
progress rejection, cancellation requests, and visible failures. Ratatui `TestBackend` fixtures
cover browser, cluster, member, and failure screens at 40×10 and 140×40. Engine integration tests
continue to verify durable retries, refresh outcomes, cluster decisions, and archive fencing.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 147 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed; TUI remains optional.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.
- Markdownlint CLI with the global config: passed, 0 issues in the changed documentation.

## P6.1 evidence

Reconciled the release description against the selected feature ledger. Native packages contain only
the Forgesync executable; cloud and portable distribution, code indexing, summaries, metrics,
analytics, legacy import, full revision history, deep PR details, and GitHub write-back remain
deferred. Forgesync keeps a separate archive format, and no Gitcrawl database or installation was
opened or changed.

Added installation and operations guidance for fresh archive setup, credential-free offline queries,
evidence coverage, interrupted-run recovery, diagnostic tracing, optional model settings, and the
TUI. Added Rust examples that call the engine's `search_threads` and `sync_repositories` APIs
directly. The CLI now installs its tracing subscriber at process startup, routes text or JSON
diagnostics to stderr, and maps `-v`, `-vv`, and `-vvv` to info, debug, and trace verbosity.

CI builds native binaries on Linux x86_64, Intel and Apple Silicon macOS, and Windows x86_64. Each
platform runs the credential-free archive/doctor/FTS5/offline-search smoke and package check. A
manual tag-based workflow packages four target-qualified archives, verifies the tag matches the
binary version, creates SHA-256 checksums, and can publish only after a user dispatches it.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 147 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.
- Local Apple Silicon release build and `scripts/smoke_binary.py`: passed; SQLite integrity, foreign
  keys, FTS5, and offline keyword search all passed without provider credentials.
- Local Apple Silicon package creation and archive member verification: passed.
- `offline_search` example run against a fresh temporary archive: passed without credentials.
- `actionlint .github/workflows/ci.yml .github/workflows/release.yml`: passed.
- `rumdl fmt .`, `rumdl fmt --check .`, and `rumdl check .`: passed, 8 Markdown files.
- Markdownlint for `README.md`, `docs/installation.md`, `docs/releasing.md`,
  `docs/compatibility.md`, and `docs/implementation-status.md`: passed, 0 issues.
- Hosted Windows, Linux, and Intel macOS jobs are configured but have not run from this checkout. No
  GitHub release has been published.

## Task sequence

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

## Rustdoc quality pass

The public module guides now explain ownership, neighboring modules, and the contracts that matter
to callers. Entry-point examples cover checked embeddings, repository selectors, redacted tokens,
and explicit archive opening. Missing field and function docs are filled in across the workspace.
The strict Rustdoc gate passes:

```sh
RUSTDOCFLAGS='-D warnings -D missing_docs -D rustdoc::broken_intra_doc_links' \
  cargo doc --workspace --no-deps --all-features --locked
```

The next documentation task is a manual review of the rendered pages with downstream users,
especially the larger workflow request and report types; lint coverage alone cannot establish
whether an explanation is sufficient.

## Source shape pass

The [cross-crate source shape audit](source-shape-audit.md) records the ownership and dispatch
review across CLI, TUI, engine, store, GitHub, and core. CLI parsing and execution now share one
private `command/` tree; archive, thread, run, search, sync, refresh, embed, and cluster commands
run through their parsed types. TUI result handling delegates by message, and engine search
delegates by retrieval path. Non-test application functions across all six crates now have source
docs explaining their purpose or invariant; trait implementations and descriptive tests use their
existing contracts. The next structural review is browser input and the larger correctness-sensitive
workflow phases.

Validation for this pass: nightly formatting check, workspace Clippy with all targets and features,
strict Rustdoc, and the CLI build without default features passed. Tests were not run for this
source-shape and documentation pass.

## Remaining maintainability audit follow-up

The earlier completion statement described the selected ownership slices too broadly. The source
shape audit now distinguishes implemented slices from remaining review surfaces. Browser navigation,
embedding policy and refresh traversal, linear test scenarios, and diagnostic query phases have
received additional cleanup. Reservation and semantic-search contracts are corrected and expanded.
Semantic candidate traversal now separates immutable scope from accumulated ranking, and changed
workflow files import dependencies at their owners. Compatible and aggressive workspace dependency
audits report no outdated dependencies.

Local validation passes: nightly formatting, workspace Clippy with warnings denied, all workspace
tests and doctests, the CLI build without default features, strict public/private Rustdoc, rumdl,
and changed-page markdownlint. Hosted platform validation remains pending. The remaining source
shape and documentation review is explicitly listed in `docs/source-shape-audit.md`; this evidence
does not claim repository-wide maintainability completion.

## Continued maintenance: child evidence and terminal input

Reservation and staging now hold exact SQL generation identity in transaction-local owners. Family
freshness distinguishes reported member counts from pull-request head evidence with an enum, and
evaluates source, head, completeness, and canonical counts in named phases. The archive caller
retains transaction commit; no provider I/O moves into the store.

Terminal global/search/triage input now dispatches to named operations. Empty-list navigation guards
remain deliberate. Parent imports that existed only to supply child input modules are removed. The
two missing production function comments found by the syntax inventory are added.

Focused evidence: all seven store observation transaction tests and all fourteen engine sync
scenarios pass. The full workspace gates then pass, including all twenty TUI tests after preserving
the empty-list navigation guards. The full completion checklist and remaining inventory are in
`docs/source-shape-audit.md`; the maintainability goal remains open.

### Crate entry documentation and CI tools

All six crate roots now teach their boundaries and lifecycle. Store, engine, GitHub, CLI, and TUI
have new compiled first-use examples; core's construction example remains in place with stronger
domain context. The examples pass workspace doctests and strict public/private Rustdoc. The six
rendered introductions were reviewed in a browser for their maps, examples, and effect boundaries.
README and installation requirements now explicitly distinguish offline keyword reads from
service-backed semantic query vectors.

The full aggressive direct/transitive dependency audit reports no outdated dependencies in all six
workspace crates. CI action versions are refreshed against upstream contracts; actionlint passes.
Local rumdl, Markdown linting, and refreshed nightly formatting pass. CI now denies missing public
docs, Rustdoc warnings, and broken links and includes private items in its documentation build. The
restored multiword keyboard-input scenario preserves the original test coverage without a loop.

Remaining work includes private item and field documentation, long function/branch review, parent
import preludes and root DTO passthrough exports, and hosted platform results. The completion
checklist remains open; this entry is evidence for the completed slices only.

Final current-tree validation for this batch passes: refreshed nightly formatting, workspace Clippy
with warnings denied, all workspace tests and doctests, the CLI build without default features,
strict public/private Rustdoc, rumdl, changed-page markdownlint, and both actionlint workflows. The
production-function comment inventory reports zero missing comments outside inherited trait
implementations; semantic documentation depth remains a separate review requirement.

## Continued maintenance: imports and search preparation

Engine-root passthrough exports of store DTOs are removed. Consumers already import the actual store
modules; the workspace compile check passes. Child-family freshness imports its domain,
serialization, archive, and observation dependencies directly. The parent module retains only its
own type dependencies, and the staged page representation documents its validation role.

Search preparation now names request conversion, fallback validation, query-client setup, and
read-only execution. Its execution owner keeps request, recipe, and optional transport together,
while retaining archive closure and cancellation. The module introduction correctly explains the
network-generated semantic query vector. All CLI unit and contract tests pass.

Outcome serialization and invalid-reference tests use named parameterized cases rather than loops.
The serialization cases still verify both exact JSON and round-trip decoding; malformed repository
paths remain a distinct scenario. These changes preserve the original assertions and inputs.

Validation for this batch passes: focused store transaction, CLI, outcome, and reference tests;
nightly formatting; workspace Clippy with warnings denied; all workspace tests and doctests; the CLI
build without default features; strict public/private Rustdoc; rumdl; and changed-page Markdown
linting. Remaining completion requirements stay open in the source-shape audit.

## Continued maintenance: retry planning and task ownership

CLI retry now uses a request owner for run/family selection. One outer method opens and closes the
archive around typed planning/client/acquisition failures, then renders the established exit status.
Sync and retry share a bounded advisory progress owner; its finish operation closes local delivery
before draining, and dropping it aborts the terminal task. Named lifecycle tests cover quiet/JSON
suppression, completion without a retained sender, and unexpected owner drop.

Engine retry planning now interprets the original request in `RecordedSelection`, eliminating the
behavioral boolean parameters. Separate methods select failures, resolve repositories in the
original fallback order, merge matching scopes, and order the result. The existing fourteen sync
scenarios pass, including the selected-family retry scenario. Named decoding cases cover missing,
malformed, valid selective, all-family, and unknown recorded scope.

Private canonical thread SQL representations move from the observation module root to their owning
column-mapping module. They document source versus evidence high-water positions and when an update
advances complete evidence. External dependencies are imported at their owners throughout the
observation implementation. All seven observation transaction scenarios pass.

Current-tree validation passes: nightly formatting, workspace Clippy with warnings denied, all
workspace tests and doctests, the CLI build without default features, strict public/private Rustdoc,
rumdl, and changed-page Markdown linting. Six recorded-selection cases and five progress lifecycle
cases pass. Generated retry, planning, progress, and SQL-update documentation was read for its
ownership and lifecycle contracts. The production-function inventory remains at zero missing
comments; broader documentation depth and source-shape requirements remain open.

## Continued maintenance: terminal picker and writer state

`RepositoryPicker` groups loaded choices, highlighted row, applied filter, and read lifecycle. It
owns beginning and applying repository reads, rejects stale results, and retains rows on failure.
Grouping exposed an applied-selection bug: the previous list index could retarget a writer when a
refresh inserted or reordered rows. Applied selection now retains the repository value instead. Four
regression scenarios cover inserted rows, empty refresh, stale failure, and failed refresh.

`OperationDisplay` owns the writer generation and an idle/running state. Running work carries a
required label and optional progress; input and the footer read those facts through named methods.
The display refuses a second writer, ignores stale completion, clears transient state on failure,
and cannot accept late progress after completion. Four direct state scenarios cover those contracts,
alongside the existing app keyboard and rendering tests. All twenty-eight TUI tests pass.

The module map and recurring selection/lifecycle rules are updated. Other terminal panels and the
remaining workspace review surfaces stay open in the source-shape audit.

Follow-up identity review also preserves renames: the selected repository's metadata refreshes only
when the provider identity matches. An absent selection retains its scope rather than selecting all.
The fifth picker regression verifies a renamed URL. The terminal state module imports engine and
store dependencies directly, and the parent no longer supplies a query-action import prelude.

The terminal coordination introduction now explains the picker/writer owners, screen and focus
semantics, message generations, initial reads, and runtime/archive boundaries. Failure-summary
fields document their safe presentation role. New state and retry-planning directory roots use
`mod.rs`; remaining older directory-root layouts are listed for review in the source-shape audit.

Final current-tree validation for this batch passes: nightly formatting, workspace Clippy with
warnings denied, all workspace tests and doctests (including twenty-nine TUI cases), the CLI build
without default features, strict public/private Rustdoc, rumdl, and changed-page Markdown linting.
Generated app, picker, and writer module documentation was read for its roles and lifecycle
contracts. The broader panel, module-layout, import, and documentation audit remains open.

## Continued maintenance: terminal read owners and typed replies

All remaining production directory roots now use `mod.rs`; obsolete explicit test paths are removed.
A workspace compile check confirms the new paths. Cargo integration-test entry files keep their
separate discovery layout.

Discussion paging now lives in `ThreadList`; `ThreadReply` retains generation, offset, and result as
one concept. `DetailPane` models empty/loading/ready/failed states, owns scroll reset, and rejects
late results after selection invalidation. Coverage and failed-run projections own their refresh,
retained-cache, error, and cursor contracts. Cluster list/detail own scoped rows and member reads.
Opening a different cluster now clears old members before acquisition; this prevents local member
keys from targeting a previous cluster while the new selection is loading. Same-cluster refresh
retains its cache, and stale replies cannot restore the previous selection.

`QueryMessage` moves to a documented result-boundary module, with contracts on every variant and
field. The app root documents its panel map and shared state. The coordinator retains cross-panel
transitions and status forwarding; standalone read starts go directly to their panel owners. Static
repository/cluster fixtures are shared by the related transition suites without hidden behavior or
provider setup.

Focused evidence: all fifty-one TUI tests pass, including five list cases, five detail cases, four
cluster-targeting/cache cases, and eight coverage/failed-run refresh cases. Existing keyboard and
viewport scenarios retain their assertions. Production-function comments remain complete by syntax
inventory. Broader query task/request, rendering import, and workspace semantic review stay open.

Current-tree workspace validation passes: nightly formatting, Clippy with warnings denied, all
workspace tests and doctests, the CLI build without default features, and strict public/private
Rustdoc. Rumdl and changed-page Markdown linting pass. Generated panel, message, coordinator, and
coverage documentation was read for its ownership, cache, generation, and archive scope contracts.

## Continued maintenance: explicit renderer dependencies

Each terminal renderer now imports domain projections and widget types from their defining crates
and modules. The view root retains only its own dependencies and shared rendering helpers; it no
longer supplies a dependency prelude to children. The compact-layout threshold documents the
readability policy it represents. Query task/request ownership remains a separate review surface.

Focused validation passes: all fifty-one TUI tests and its doctest, plus all-target/all-feature
Clippy with warnings denied. The broader workspace gates passed immediately before this import-only
change.

## Continued maintenance: query task lifecycle

`query/tasks` now owns read handles, writer cancellation, and shutdown draining. Dispatch requests
cancellation through its method, and writer spawning registers a handle/token pair without changing
the owner's fields. Module and item documentation distinguish explicit shutdown (which awaits
cleanup) from drop (which can only signal cancellation). Request shapes and operation progress
forwarding remain separate review work.

All fifty-four TUI tests and its doctest pass, including three task-lifecycle cases. Focused Clippy,
strict public/private Rustdoc, nightly formatting, rumdl, and changed-page Markdown linting pass.
The source audit remains open for request ownership and the remaining workspace review surfaces.

## Continued maintenance: named terminal requests

`query/requests` documents each intent variant and field, repository scope, read generation, and
local versus provider-backed effects. Dismiss/restore and exclude/include are separate variants
instead of boolean-selected commands. All callers import the request owner directly. Two nearby
triage tests assert restore and include requests, including the exact member selector. All fifty-six
TUI tests and its doctest pass; focused Clippy and strict public/private Rustdoc also pass.

## Continued maintenance: failure-ledger presentation

The query root now dispatches requests without carrying run-history scanning or string formatting.
`query/failures` owns the bounded recent-run selection and isolated detail failure projection.
`RunFailureSummary::from` owns its detailed-ledger conversion, documenting job/failure ordering,
omission of completed/resolved rows, and the separation from retry eligibility. Static ledger tests
cover selection order, the twenty-detail bound, unreadable-detail identity, and presentation rows.

All sixty-one TUI tests and its doctest pass. Focused Clippy with warnings denied, strict
public/private Rustdoc, nightly formatting, rumdl, and changed-page Markdown linting pass.

## Continued maintenance: discussion page request ownership

`ThreadRead` now owns browser/keyword page preparation in `query/thread_page`. Its fields document
submitted query, applied scope, and reply offset; methods name the two retrieval paths and common
filters. The async starter owns only generation, task spawning, and typed reply delivery. Named
filter cases verify that both ranking policies retain repository scope and page bounds.

All sixty-three TUI tests and its doctest pass. Focused Clippy with warnings denied and strict
public/private Rustdoc pass; nightly formatting is applied.

## Continued maintenance: local read dispatch context

`ReadDispatch` binds the archive, runtime, result channel, and task lifetime owner used by every
local read. Starters are methods with only their panel/request inputs; their shared service
parameters no longer recur across signatures or dispatch calls. Provider clients remain outside this
context. Documentation names pending-state timing, owned task clones, discarded delivery after
channel closure, and shutdown abortion of side-effect-free reads.

All sixty-three TUI tests and its doctest pass after this dispatch change. Focused Clippy with
warnings denied and strict public/private Rustdoc pass. Syntax inventory confirms discussion
preparation and read starters no longer contain long, mixed search/scheduling bodies.

## Continued maintenance: terminal progress delivery lifetime

`query/progress` owns the engine producer and forwarding task for a writer generation. Normal
completion closes production and drains progress before the terminal result. Unexpected owner drop
aborts delivery, so a writer exit cannot detach a receiver indefinitely. Four channel-level cases
cover ordering, empty draining, producer closure on drop, and closed terminal delivery. The writer
starter now follows admission, execution, progress drain, and completion rather than containing the
forwarder's receive loop.

Current-tree validation passes: all sixty-seven TUI tests and its doctest, full workspace tests and
doctests, workspace Clippy with warnings denied, the CLI build without default features, nightly
formatting, strict public/private Rustdoc, rumdl, and changed-page Markdown linting. Generated
query, request, failure selection, discussion-page, read-dispatch, and progress documentation was
read for its contracts. The broader audit remains open: ordinary production-function comments are
present, but forty-one trait implementation methods still need review for meaningful local contract
docs, alongside the existing private-item, function-shape, and test-readability targets.

## Continued maintenance: CLI sync ownership

`SyncArgs` now owns scope conversion and acquisition. Its outer execution closes the archive once
after successful opening, before rendering either a report or a typed selection/client/engine
failure. Client preparation and progress draining remain explicit phases. The module uses ordinary
owner imports and `mod.rs` with nearby request-conversion tests. Five named scope/family cases and
all three existing CLI sync contract cases pass; focused Clippy and strict Rustdoc pass.

## Continued maintenance: provider setup error causes

Shared GitHub setup failures now implement standard error traits and retain credential, URL, and
adapter causes instead of formatting them during preparation. Sync/retry failure boundaries retain
those sources through resource cleanup; rendering keeps the established codes, messages, and exit
policy. The setup module documents host deduplication, anonymous fallback, and cancellation. Its
nearby source-contract tests use static errors without process or network fixtures.

All four static source-contract cases pass. Current-tree workspace validation passes: Clippy with
warnings denied, all workspace tests and doctests, the CLI build without default features, strict
public/private Rustdoc, nightly formatting, rumdl, and changed-page Markdown linting. The broader
source review remains open, including command-root aliases and trait implementation contracts.

## Continued maintenance: command type import ownership

The private CLI command root no longer re-exports child argument types or parsing choices. It
imports only types used by its own parser and dispatcher; feature implementations and tests import
choices from `command/values` and cluster arguments from `command/cluster`. The process entry point
also names the values owner directly. Module visibility retains the private command boundary.

All-target/all-feature compilation and all thirty-three CLI unit tests pass after the import change.
The remaining report facade and its unrelated output DTOs are the next ownership review surface.

## Continued maintenance: report module ownership

Report formatters are imported through their workflow modules rather than a root export facade.
Embedding, cluster-decision, migration, and retry-interruption DTOs now live with their presentation
code. Each DTO and field documents its process meaning and separation from domain/store evidence.
The retry-only interruption payload has a descriptive name, retaining its JSON fields. Module
introductions explain these relationships and the embedding summary's failure precedence.

All thirty-five CLI unit tests and fourteen CLI contract tests pass. Workspace Clippy, all workspace
tests/doctests, the build without default features, strict public/private Rustdoc, rumdl, and
changed-page Markdown linting pass. DTO relocation preserves the serialized output contract.

## Continued maintenance: refresh stage presentation

The long refresh summary match now selects named stage presentation and payload-count functions. The
existing `RefreshStage<T>` keeps status, optional report, and failure together; a small shared
formatter renders those facts consistently without introducing a second state wrapper. Nearby linear
tests verify absent payloads, safe failures, selected order, omitted stage records, and the
remaining-work suffix.

All four refresh presentation cases and thirty-nine CLI unit tests pass, including partial payload
counts before a stage failure. Full workspace Clippy, tests/doctests, the build without default
features, strict public/private Rustdoc, rumdl, and changed-page Markdown linting pass.

## Continued maintenance: local trait contracts

All forty-one previously undocumented handwritten trait methods now explain their local contracts.
The pass covers identity and timestamp serialization/validation, configuration defaults, JSON
projection, parsing, deterministic heap ordering, safe error conversion, credential redaction, and
background-task cleanup. These comments describe choices the standard trait contract cannot convey.
The documentation guide now records this distinction for future changes.

Strict public/private workspace Rustdoc passes with warnings and missing public documentation
denied. The syntax inventory finds no undocumented handwritten production methods, including trait
methods; this establishes presence, not completion of the deeper documentation review. Private
representations, field contracts, policy constants, and remaining workflow shape remain open review
targets.

## Continued maintenance: sync accounting contracts

The internal sync scope, shared execution capabilities, accumulated work counters, and per-thread
family results now explain their attribution, ownership, and partial-work meaning. Field comments
distinguish received from committed evidence, terminal from successful jobs, and interrupted work
from satisfied scope. The public progress and report contracts now explain that the job denominator
can grow when a nonempty repository scope adds pull-request family jobs; it is not a fixed count of
threads. The lease renewal interval and closed-sweep overlap also have local policy explanations.

The comments were checked against job creation, interruption accounting, review collection
completion, and outcome selection. All fourteen sync workflow scenarios and strict private/public
engine Rustdoc pass. Nightly formatting and Markdown checks pass. The remaining sync coordinator and
representation ownership still need structural review; these contracts establish what a subsequent
extraction must preserve.

## Continued maintenance: owned sync outcome accounting

`sync/accounting` now owns `WorkSummary` and its failure classification, terminal family accounting,
remaining-work calculation, and outcome selection. Those methods previously lived across the root,
job runner, family job, and support module. Callers now import the representation from its owner and
ask it to record failures or select an outcome. The support module imports domain, store, and engine
types directly instead of relying on a parent import prelude. Its responsibility is attribution and
presentation of individual job evidence.

Five nearby linear unit scenarios pass for complete, partial, deferred, failed, and interrupted
outcomes, including first-diagnostic retention and interrupted jobs remaining retryable. The
repository/provider integration scenarios remain the evidence for durable acquisition behavior. The
broader coordinator, other representations, and remaining workflow candidates are still open.

Current-tree validation for this extraction passes: five focused accounting cases, fourteen sync
workflow scenarios, workspace Clippy with warnings denied, all workspace tests/doctests, the CLI
build without default features, strict public/private Rustdoc, nightly formatting, rumdl, and
changed-page Markdown linting. The generated accounting module and type documentation were checked
for the ownership and counter distinctions above.

## Continued maintenance: sync preparation and lease lifetime

The sync coordinator now presents repository preparation, scope accounting, fence acquisition,
durable run creation, and execution as named phases. The request owns validation, deduplication,
client availability checks, initial job counting, and its recorded scope. `SyncLease` owns
acquisition, run-creation failure cleanup, renewal, child cancellation, draining, and release.
Renewal failure still waits for job cleanup rather than dropping the acquisition future. Run-summary
failure recording is a method on the existing run context, preserving the original acquisition
error.

A direct integration scenario forces run creation to fail through a missing parent run. It verifies
no run was persisted, the caller was not cancelled, and the fence can be reacquired. Reacquisition
uses a timestamp too old to expire a leaked current-time lease, so it establishes release rather
than merely observing expiry. The selected sync workflow scenarios continue to cover committed
partial evidence, family failure isolation, closed-sweep checkpoints, and cancellation.

The remaining private representations and other workflow candidates stay in the full source audit;
this extraction does not establish completion of those surfaces.

Current-tree gates pass: fifteen sync workflow scenarios, workspace Clippy with warnings denied, all
workspace tests/doctests, the CLI build without default features, strict public/private Rustdoc,
nightly formatting, rumdl, and changed-page Markdown linting. The generated lease module/type docs
were checked for writer ownership, cooperative cleanup, and release-policy distinctions.

## Continued maintenance: clustering representation ownership

Neighbor heaps and candidate edges now live with evidence selection; proposed clusters and members
live with bounded component construction. Private representations use public items inside private
modules rather than exposing crate-only models at the public clustering root. Mention regexes,
worker permits, page sizes, and lease lifetime now live at their behavioral owners. Production child
imports name store, runtime, and engine owners directly instead of depending on a parent import
prelude.

Type and field contracts explain stable snapshot indexes, heap ordering, transitive membership,
proposal-versus-maintainer decisions, and vector-versus-reference weights. Constants explain their
policy role, including byte offsets and non-probabilistic heuristic scores. Options validation is
available on its public owning type, with explicit limits and a runnable customization example. The
four candidate-policy cases and cluster-generation integration scenario pass; workspace Clippy
passes. Deeper algorithm shape and the other private representations remain review targets.

Current-tree workspace tests and doctests pass, including the new public options validation example.
The CLI build without default features, strict public/private Rustdoc, nightly formatting, rumdl,
and changed-page Markdown linting also pass. Generated options, member-proposal, and edge docs were
checked for threshold limits and the distinction between heuristic weights and durable identities.

## Continued maintenance: cluster proposal projection

The long component-formatting closure is now a focused `proposals` module. `ClusterProjection` keeps
the immutable document snapshot, retained degrees, and direct edge weights together, preparing those
indexes once for every component. Named methods project a group, rank its representative, and attach
member scores. Bounded union-find grouping stays in `components`, with its own input/order contract.
No vector rescoring, threshold, ordering, or persistence policy changed.

Five nearby linear projection cases establish degree selection, numeric identity ties, absent direct
scores for transitive membership, singleton self scores, and minimum-size filtering. The existing
determinism case compares complete proposals directly and asserts group counts/sizes; its test-only
projection closure and generated input loop are removed. A shared static document fixture only
constructs explicit scenario facts and has no branching, provider acquisition, or assertions. All
nine clustering unit cases pass. The other workflow and test-shape candidates remain open in the
full audit.

Final current-tree validation passes: all nine clustering unit cases, the cluster-generation
integration case, workspace Clippy with warnings denied, all workspace tests/doctests, the CLI build
without default features, strict public/private Rustdoc, nightly formatting, rumdl, and changed-page
Markdown linting. Generated proposal module, projection type, and function docs were checked for
stable-index contracts and the direct-score distinction. The next concrete workflow review is CLI
refresh preparation and archive cleanup, alongside the full remaining source checklist.

## Continued maintenance: refresh preparation and archive lifetime

CLI refresh no longer combines argument destructuring, credential setup, optional model capability
selection, scope construction, execution, cleanup, and presentation in one long function. Parsed
arguments own validation, client selection, and sync-option conversion. `PreparedRefresh` keeps the
ordered engine request with its selected embedding capability and executes it against explicit
services. The outer command closes the archive once, then renders the report or a typed source
error.

Six nearby linear cases pass for cluster-only capability selection, explicit sync families/state,
stage order and forced embedding with unusable optional configuration, duplicate-stage validation,
engine source retention, and local-only client setup with cancellation already signalled.
Cluster-only preparation does not request an embedding client. Invalid optional configuration still
leaves its selected analysis stage present for structured partial outcomes. Workspace Clippy passes.
Remaining CLI embedding and cluster-build workflow candidates stay in the full source review.

Final validation passes: six refresh preparation/boundary cases, all forty-five CLI unit tests,
workspace Clippy with warnings denied, all workspace tests/doctests, the CLI build without default
features, strict public/private Rustdoc, nightly formatting, rumdl, and changed-page Markdown
linting. Generated refresh module, prepared-request, and typed-failure documentation were checked
for stage capability ownership and cleanup ordering. The next concrete command workflow is embedding
preparation and its report/failure presentation.

## Continued maintenance: embedding outcome presentation

The embedding command delegates missing-report presentation to a named operation after archive
closure. Its diagnostic exit policy uses the stable cancellation code rather than message text.
Report-bearing exit policy is a query on `EmbeddingOutput`, beside the status serialized and
rendered for users, instead of another presentation match inside acquisition.

Two direct diagnostic cases verify interruption and message-independent fatal classification. Five
named rstest cases verify complete, partial, deferred, interrupted, and failed report states without
loops or scenario helpers. Reportless failure and report-bearing retryable work remain distinct. The
broader embedding preparation and engine batch workflows are still open review targets.

Final local gates pass: both diagnostic cases, all five report-state cases, all fifty-two CLI unit
tests, workspace Clippy with warnings denied, all workspace tests/doctests, the CLI build without
default features, strict public/private Rustdoc, nightly formatting, rumdl, and changed-page
Markdown linting. Embedding request preparation remains the next concrete command surface.

## Continued maintenance: prepared embedding execution

`PreparedEmbedding` keeps canonical repository scope, validated service client, document recipe,
typed cache/replacement policy, and configured dimensions together through acquisition and output
projection. Parsed arguments resolve overrides and client setup before archive opening. The command
then opens, runs, closes, and presents one concrete report or safe stage diagnostic. Request
identity is attached by the same owner that executed it; missing reports no longer propagate
optional errors.

Three additional nearby cases establish unique URL ordering, the concrete missing-report fallback,
and preservation of partial output identity/counts/failure. Static client setup supplies its test
key explicitly and performs no provider requests or process credential mutation. The command
converts its parsed force flag directly into a typed policy instead of passing a behavioral boolean
onward. The engine's remaining embedding scheduling and related workflow candidates remain open.

Current-tree gates pass: five embedding command cases, all fifty-five CLI unit tests, workspace
Clippy with warnings denied, all workspace tests/doctests, the CLI build without default features,
strict public/private Rustdoc, nightly formatting, rumdl, and changed-page Markdown linting.
Generated embedding module and prepared-execution docs were checked for identity ownership and
cleanup responsibilities. The full audit remains open, including engine batch coordination and other
private representations.

## Continued maintenance: deterministic embedding chunks

Embedding text splitting, versioned chunk identity, and persisted-vector compatibility now live in
`embeddings/chunks`. The workflow retains scheduling and fenced persistence; nearby chunk tests
exercise deterministic values and oversized multibyte input separately from request batching.
Private task and batch fields explain retained source identity and service-response ordering.

Focused embedding cases, workspace Clippy, workspace tests/doctests, the CLI build without default
features, strict public/private Rustdoc, and nightly formatting pass. Batch selection, scheduling,
and lease ownership remain the next embedding execution work.

## Continued maintenance: embedding document selection

`EmbeddingSelection` now owns source-version deduplication, current chunk selection, stored-vector
reuse, and selected/skipped accounting. `EmbeddingTask` lives beside the phase that constructs it
and retains the full source document needed for fenced persistence. The coordinator reads as
selection followed by lease acquisition, execution, and release. Selection remains read-only and
precedes the writer lease; replacement preserves the existing archive-read error contract.

Focused embedding cases pass, including the integration retry scenario that retains successful
batches and requests only missing chunks. Scheduling and persistence ownership remain open in the
embedding execution batch.

Current-tree workspace Clippy, all workspace tests/doctests, the CLI build without default features,
strict public/private Rustdoc, nightly formatting, and changed-page rumdl/Markdown linting pass.

## Continued maintenance: embedding worker and writer lifetimes

The coordinator now selects documents, returns immediately when no requests are needed, and
delegates the active writer lifetime to `EmbeddingWriter`. Its service identity and fence travel
together through persistence. `BatchScheduler` owns pending batches, active workers, outcome
dispatch, and cleanup; `EmbeddingBatch` owns request input/response order. The production files are
coherent leaves under the same embedding directory rather than one combined workflow body.

Fatal worker and persistence exits now explicitly abort and drain remaining workers before releasing
the writer fence. Previously those exits relied on dropping the worker set, which requests abort
without waiting for request resources to drop. Two direct lifetime cases establish resource release
on fatal completion and cancellation, preserving the original error and completed chunk count. Named
batch cases distinguish count limits, byte limits, and their combination without generated scenario
loops or nested assertions. Existing integration retry retains successful chunks.

The focused run passes seven embedding unit cases and the partial-success/retry integration case.
The embedding execution batch has its named phase owners and cleanup evidence; the cross-workspace
documentation, signature, and test acceptance passes remain separate requirements.

Final current-tree gates pass: workspace Clippy with warnings denied, all workspace tests/doctests,
the CLI build without default features, strict public/private Rustdoc, nightly formatting, workspace
rumdl, and all four changed Markdown pages. The module map and source audit record embedding
execution as implemented. Cluster construction is the next bounded batch; seven implementation
batches and the final acceptance pass remain.

## Continued maintenance: cluster build lease coordination

`ClusterBuildLease` now owns the generation fence, heartbeat policy, child cancellation scope, and
completion protocol. The public build operation validates its request, acquires the owner,
constructs the analysis future, and asks the owner to complete it. Renewal and interruption helpers
remain nearby so their cleanup ordering can be read without following the generation projection.

Two named failure cases prove release by reclaiming the fence at an epoch timestamp, which cannot
mask a leaked current lease through expiry. A separate interruption case proves cooperative cleanup,
preservation of the triggering error, and isolation from the caller token. The generation workflow
integration case still passes for current open vectors and complete-coverage retirement. This is a
part of the cluster construction batch; request preparation, vector traversal, and generation
persistence still require their bounded review.

Validation passes for the three new lease cases, existing generation integration, final-tree
workspace Clippy and all workspace tests/doctests, the CLI build without default features, strict
public/private Rustdoc, nightly formatting, workspace rumdl, and both changed Markdown pages. The
next cluster surface is CLI build request preparation and result presentation.

## Continued maintenance: CLI cluster build preparation

Parsed `ClusterBuildArgs` now owns build execution and request conversion. Preparation applies
service overrides, validates configuration, canonicalizes endpoint/model identity, and preserves
recipe and graph policy without reading credentials or contacting a model service. The engine keeps
policy validation and generation behavior. The outer command opens, executes, closes, and presents
through named operations rather than inline multi-effect match arms.

Three nearby linear cases pass for canonical service overrides, all graph policy values, and
rejection of nonlocal HTTP identity before archive execution. Cluster construction remains open for
refresh traversal, engine evidence loading/projection, and store generation persistence.

Final current-tree validation passes: all three preparation cases, all fifty-eight CLI unit tests,
workspace Clippy, all workspace tests/doctests, the CLI build without default features, strict
public/private Rustdoc, nightly formatting, workspace rumdl, and changed-page Markdown linting. The
next cluster work is refresh traversal and engine evidence preparation, followed by the store's
transactional generation application and identity matching.

## Continued maintenance: refresh cluster stage accounting

`ClusterStage` now owns shared build policy and accumulated repository outcomes. The public stage
operation traverses repositories, delegates attempts, and finishes one report. Separate methods
construct requests, record successful coverage, retain failures, and account for interruption.
Imports name defining modules directly instead of using the refresh parent's dependency prelude;
unused parent-only imports are removed.

Eight nearby named cases cover empty scope, complete generation, partial coverage, missing identity,
cancellation as primary diagnostic, cancellation before an attempt, success retained through a later
failure, and preservation of an earlier primary diagnostic through interruption. Static fixtures
construct values only; no scenario loops or archive/provider setup hide the accounting rules.
Refresh traversal is implemented; engine evidence preparation and transactional store generation
application remain in the cluster construction batch.

Final current-tree gates pass: the eight accounting cases, workspace Clippy with warnings denied,
all workspace tests/doctests, the CLI build without default features, strict public/private Rustdoc,
nightly formatting, workspace rumdl, and all changed Markdown pages. Engine evidence preparation and
store generation application are the next concrete cluster surfaces.

## Continued maintenance: cluster vector snapshot preparation

`ClusterSnapshot` now owns repository resolution, eligible open-thread counting, compatible-vector
traversal, and coverage validation. It retains source and vector counts independently so incomplete
coverage remains a useful generation without permission to retire unseen clusters. Vector paging
uses the existing build request for endpoint/model/recipe identity instead of six scalar parameters.
Read queries are named before archive calls, and page count conversion is separate from checked
accumulation.

The generation coordinator now follows evidence loading, bounded analysis, store-input projection,
and fenced persistence. A named candidate conversion keeps nested membership construction out of the
workflow. Expanded build docs explain local-only inputs, lease/cancellation ordering, unavailable
vectors, partial coverage, and transaction ownership. Existing generation integration passes for
complete coverage, partial coverage, unavailable vectors, and retirement behavior.

Engine evidence preparation is implemented. The remaining cluster construction surface is the
store's transactional generation application, row preparation, and durable identity matching.

Final current-tree gates pass: generation integration, workspace Clippy, all workspace
tests/doctests, the CLI build without default features, strict public/private Rustdoc, nightly
formatting, workspace rumdl, and changed-page Markdown linting. The snapshot and coordinator
introductions now describe their separate read/analysis/write roles. Store generation application
and identity matching remain the final implementation surface of the cluster construction batch.

## Continued maintenance: store generation preparation boundaries

Generation validation and SQL identity resolution now live in `generation_input`; durable overlap
matching lives in `generation_matching`. Their representations have local field contracts rather
than private declarations on the cluster API root. Decision operations import the shared row
resolver from its defining module. Preparation docs now correctly distinguish validation before
transaction creation from row resolution within the transaction.

The generation transaction and SQL algorithms remain unchanged. Write orchestration, repeated run
identity/timestamp parameters, and overlap-ranking representation remain the next store review
targets before the cluster construction batch can be closed.

Current-tree validation passes: workspace Clippy, all workspace tests/doctests, the CLI build
without default features, strict public/private Rustdoc, nightly formatting, workspace rumdl, and
changed Markdown linting. The public cluster page and decision entry points retain their existing
API; only private generation representations moved. The cluster construction batch remains open for
matching evidence representation and transaction write orchestration.

## Continued maintenance: durable cluster matching evidence

`MembershipOverlap` replaces the four-position tuple used for durable identity assignment. Its
comparison names absolute overlap, proportional overlap, existing row identity, and generated
position. Named candidate enumeration and greedy assignment keep traversal separate from ranking;
exact cross multiplication preserves the existing fraction comparison without floating-point error.

Six nearby linear scenarios establish strongest absolute overlap despite a lower proportional score,
proportional ties, stable identity and generated-position ties, one-to-one assignment, and disjoint
membership. Fixture constructors only construct explicit membership representations. Transactional
write orchestration remains the final cluster construction surface.

Final current-tree gates pass: all six matching cases, workspace Clippy, all workspace
tests/doctests, the CLI build without default features, strict public/private Rustdoc, nightly
formatting, workspace rumdl, and changed-page Markdown linting. Generation write orchestration
remains the next concrete cluster surface; the matching rule and durable identity ordering are
preserved.

## Continued maintenance: cluster generation application state

`GenerationApplication` owns the facts shared by generation writes and their accumulated outcomes.
The archive entry validates, opens/fences the transaction, resolves rows, matches durable identity,
applies membership, and commits once. Per-cluster application names identity upsert, membership
movement, complete-coverage removal, generated member writes, and event insertion. Retirement and
run finalization occur after all groups. Result conversion retains its original post-commit timing.

SQL helpers live in `generation_rows` with explicit column/value correspondence. Their broad
signatures are retained as bounded SQL mutation inputs rather than hidden generic write contexts;
the application owner supplies shared run/repository/time facts consistently. The leaf is roughly
250 lines because its bind maps are linear, not because it contains another coordinator.

The store generation suite covers partial preservation, complete retirement, durable decisions, and
invalid coverage rejection. The engine generation suite covers current vectors and
coverage-dependent retirement. Cluster construction now has coherent preparation, analysis, lease,
and persistence owners; remaining workspace-wide signatures/docs/test review stays in its respective
bounded batch.

Final current-tree gates pass: workspace Clippy, all workspace tests/doctests including store and
engine generation contracts, the CLI build without default features, strict public/private Rustdoc,
nightly formatting, workspace rumdl, and changed-page Markdown linting. The first two bounded
implementation batches are implemented. Six implementation batches and final acceptance remain;
search ranking/retrieval is next.

## Continued maintenance: hybrid search rank fusion

Hybrid fusion now has a private module and local owners for the identity union and per-discussion
evidence. Semantic rank and cosine score are one value instead of independently optional fields.
Source merging, hit projection, requested ordering, stable ties, and truncation are named operations
rather than one long function with an inline type and substantial projection closure.

Ranking imports its actual result/error owners; fusion imports domain identity, summary, and
ordering from their defining modules. The rank smoothing constant now lives beside its formula with
its meaning documented. Keyword-first summary precedence, last duplicate source-rank behavior,
source provenance order, score formula, stable ordering, and candidate truncation remain unchanged.
Retrieval orchestration remains open in the search batch.

Hybrid fusion passes workspace Clippy, all workspace tests/doctests, the CLI build without default
features, strict public/private Rustdoc, nightly formatting, rumdl, and changed-page Markdown
linting.

## Continued maintenance: ranked search window

`search::window::SearchWindow` owns validated presentation coordinates and the candidate prefix
needed for ranked pagination. It includes skipped results and one continuation probe, preserving the
10,000-result ranking budget. Retrieval consumes the same window for candidate acquisition and page
construction. Keyword-only pagination remains at the store boundary.

Four nearby linear boundary cases cover ordinary paging, the maximum prefix, exceeding the budget,
and the shared invalid-offset classification. The latter exposed an incorrect initial test
expectation: shared pagination validation rejects offsets above SQLite's signed range before ranked
window validation. The implementation preserves that existing error precedence.

Retrieval orchestration and the remaining six-batch work are still open. Current-tree gates pass:
workspace Clippy, all workspace tests/doctests, the CLI build without default features, strict
public/private Rustdoc, nightly formatting, rumdl, and changed-page Markdown linting.

## Continued maintenance: ranked retrieval orchestration

Ranked acquisition and result construction now live in `search::ranked` instead of the public search
module. `RankedSearch` retains the validated request/window relationship through semantic
acquisition and projection. Its dispatch delegates to named cosine and hybrid projections, and its
candidate method makes the unavailable-service boundary explicit before scoring.

Acquisition order remains keyword candidates, semantic evidence, then successful-result coverage.
Hybrid fallback still reuses keyword candidates; semantic fallback performs a local keyword read.
Existing cancellation, provider-error classification, source ranking, page offsets, and JSON DTOs
remain unchanged. Archive, client, recipe, and cancellation stay explicit operation inputs rather
than being hidden in a generic application context. Workspace Clippy and all engine tests/doctests
pass. The completed search validation also passes all workspace tests/doctests, the CLI build
without default features, strict public/private Rustdoc, formatting, rumdl, and changed-page
Markdown linting. Scoring limits now live beside their implementation, and keyword/semantic helpers
import dependencies from their actual defining modules. The first three bounded batches are
implemented; acquisition is next, with five implementation batches and final acceptance remaining.

## Continued maintenance: metadata completion phases

`MetadataObservation::complete` now names staging, canonical application, and failure resolution
before constructing the dependent-family result. `apply` accepts only applied/replayed dispositions;
a superseded reservation still returns the existing stale-generation error. Failed acquisition names
its incomplete terminal write before cancellation or diagnostic handling. Private fields explain the
archive/provider split, parent source clock, run fence, and reserved generation.

The archive commits and ledger order remain unchanged. A failure-resolution error still returns an
error after canonical metadata has independently committed. Review collectors receive no successful
result from that attempt. The existing sync scenarios cover metadata-dependent reviews, changed
heads, partial snapshots, and retry behavior. The final metadata tree passes workspace Clippy, all
15 sync workflow scenarios, nightly formatting, rumdl, and changed-page Markdown linting.
Acquisition scan and store finalization remain open in the bounded acquisition batch.

## Continued maintenance: repository scan persistence

Repository provider traversal, durable scan writes, and terminal meaning now live in separate
private modules. `ScanPersistence` uses its borrowed reservation directly instead of repeatedly
cloning/destructuring the full context. Terminal recording precedes the durable report read in a
short `finish` method. `ScanOutcome` documents and maps complete, interrupted, and failed
acquisition without conflating cancellation with a provider diagnostic.

Three nearby linear cases cover complete coverage, cancellation without failure evidence, and safe
network-failure classification. Existing integration cases cover page-two failure retaining page one
and replay avoiding duplicate canonical rows. Page application, cursor advancement, optional fence
checks, clock evaluation, and incomplete coverage retain their existing ordering. Validation is in
progress; store scan finalization and replay-scenario review remain in the acquisition batch.

## Continued maintenance: typed store scan completion

Store finalization validates `ScanCompletion` before opening its transaction. Complete coverage
cannot carry failure evidence, while incomplete coverage can represent interruption or retain a safe
diagnostic. A private completion module owns the active-generation/cursor guard and terminal SQL
write. The archive method opens/fences, validates the cursor, writes, and commits in reading order.

The SQL predicates, binding order, missing-generation error, pending-cursor rejection, failure
serialization timing, and single-commit boundary are preserved. Four nearby linear cases cover
active-status rejection, failure-bearing completion rejection, complete coverage without diagnostic,
and interruption without provider failure. Existing enumeration integration scenarios exercise the
transactional path. All four completion cases, both enumeration integration cases, workspace Clippy,
formatting, rumdl, and changed-page Markdown linting pass. Full workspace validation is running; the
acquisition batch remains open.

## Continued maintenance: linear enumeration replay

The replay integration scenario now explicitly performs and checks its initial acquisition and
replay instead of looping over the operation. It also checks that replay reserves a newer
acquisition sequence while retaining two canonical threads. The renamed-repository scenario sets up
its redirect inline; the repository fixture supplies only a static successful response, without
hidden transport branching. Fixture and archive-cleanup helpers document their limited roles.

Both enumeration integration scenarios pass. Store completion now passes all workspace
tests/doctests, the CLI build without default features, strict public/private Rustdoc, workspace
Clippy, and formatting. Further checkpoint-rejection cases and the acquisition acceptance review
remain open.

## Continued maintenance: checkpoint completion integration

A direct store scenario exercises premature completion before the initial cursor has cleared. It
checks that rejection leaves the active status, page count, and cursor unchanged, then records an
empty terminal page and completes successfully with one page and zero threads. Every transition is
visible in the scenario; no provider fixture or behavioral helper establishes the state implicitly.
The integration case, workspace Clippy, nightly formatting, rumdl, and changed-page Markdown linting
pass. Superseded-generation coverage and acquisition acceptance remain open.

## Continued maintenance: superseded scan isolation

A second direct store scenario records an old generation's terminal page, starts a newer scan, and
tries to finalize the old generation. It expects the existing missing-active-generation error and
checks the new sequence, cursor, zero counts, active status, and absent failure remain unchanged.
The shared repository fixture builds only static data; every acquisition transition stays visible.

Acquisition implementation review now covers traversal versus persistence, explicit terminal
coverage, metadata staging/application/resolution, and linear failure/replay/checkpoint cases. The
scan traversal remains a linear provider/apply/cursor loop. Fenced/unfenced store dispatch remains
an explicit choice of one archive call per branch; additional forwarding abstractions would add
navigation without changing the concept. Metadata reservation retains its linear ordering and SQL
observation projection. Final acquisition validation is in progress.

## Continued maintenance: aggregate coverage projections

Archive coverage totals and archive status now live in `reads::summary`; per-discussion coverage and
staleness remain in `reads::coverage`. `FamilyCoverageSummary` owns checked bucket accumulation and
stored-status validation. The leaf modules import actual external dependency owners, and obsolete
root dependency aliases are removed.

The SQL grouping, family applicability, count validation, bucket assignment, overflow behavior, and
query ordering remain unchanged. Module guidance explains missing evidence and the separate read
snapshots used by aggregate status. Three nearby linear cases cover bucket/denominator accounting,
negative counts, and unsupported status labels. The small fixture creates only static zero counts.
The three accumulation cases, four inspection integration cases, workspace Clippy, rumdl, and
changed-page Markdown linting pass. Final workspace validation is running; store-operation review
remains open.

Final acquisition gates pass: all workspace tests/doctests, the CLI build without default features,
strict public/private Rustdoc, workspace Clippy, nightly formatting, rumdl, and changed-page
Markdown linting. The first four bounded batches are implemented; store operations, presentation,
workspace conventions/docs, test-suite review, and final acceptance remain.

## Continued maintenance: prepared document persistence

`DocumentUpdate` binds the validated domain document, resolved thread row, serialized source
identity, and build time for one persistence operation. The archive entry now exposes validation,
transaction/fence, parent resolution, projection preparation, persistence, and commit. The
projection names prior hash comparison, the explicit upsert map, and unchanged-row ID lookup.

Upsert SQL is byte-for-byte identical to the prior change. Bind order, validation-before-writer
precedence, parent-resolution-before-serialization precedence, source-clock-only build-time
preservation, unchanged-row identity, hash-change reporting, and commit timing are retained. The SQL
column map stays linear rather than introducing helpers per field. Workspace Clippy passes; document
materialization scenarios and final workspace gates are running. Store-operation review remains
open.

Aggregate coverage final gates pass: all workspace tests/doctests, the CLI build without default
features, strict public/private Rustdoc, workspace Clippy, formatting, rumdl, and changed-page
Markdown linting.

## Continued maintenance: discussion timeline projection

Timeline projection now lives in `reads::timeline` rather than the archive detail assembler. Named
entry constructors keep event payload and timestamp semantics together. Review-thread state remains
undated; its comments retain source creation times. The coordinator appends selected evidence and
applies a named comparator, preserving known-time-first ordering and stable event identity keys.

Detail assembly imports actual dependency owners, and obsolete parent aliases are removed. Its
existing integration case checks creation/comment timeline order with selected membership and
coverage. Two nearby cases make dated/undated ordering explicit. Store inspection integration and
workspace Clippy pass. Document-write workspace doctests encountered the intermediate timeline
extraction during compilation; that temporary import error is corrected. Final validation runs
against the corrected combined tree. Store-operation review remains open.

## Continued maintenance: diagnostic read contracts

Diagnostic documentation now explains that schema inspection rejects invalid migration history
before returning a record, so successful `history_valid` values are always true. Lease `held` is a
momentary observation, not a fencing capability. Section/counter reads are independent rather than a
frozen snapshot, and the unresolved total can include family labels this binary does not recognize.

The public diagnostic operation documents read order, errors, and absence of repair effects. This
change modifies documentation only; existing linear count queries and simple status mappings remain
locally readable. Both new timeline ordering cases pass in the combined workspace run. Final
combined workspace gates remain running. Cluster member-decision mutations are the next store review
surface.

The corrected combined document/timeline/diagnostic tree passes all workspace tests/doctests, the
CLI build without default features, strict public/private Rustdoc, workspace Clippy, nightly
formatting, rumdl, and changed-page Markdown linting. Store-operation cleanup remains open.

## Continued maintenance: prepared cluster member decisions

`MemberDecisionWrite` retains the resolved cluster/member, typed inclusion choice, reason, and
action time through durable decision recording, membership update, canonical cleanup, and event
insertion. The archive entry preserves early reason/ID validation, transaction/fence ownership,
current-member resolution, and one final commit. Named policy values replace positional
exclusion/state/event coordinates, keeping those encodings coupled and readable at their bind sites.

Member-decision SQL statements are unchanged. Bind order, trimming, canonical cleanup only on
exclusion, event labels, and mutation order are preserved. The new private module documents the
transaction boundary and the distinction between local choices and generated source evidence.
External dependencies import from their actual owners; unused root aliases are removed. Both cluster
integration cases, formatting, rumdl, and changed-page Markdown linting pass. Final current-tree
Clippy and workspace validation are running; cluster decisions and queries remain in the bounded
store-operation review.

## Continued maintenance: cluster inspection contracts

Cluster read documentation now explains effective representative selection, generation-derived
titles, lifecycle versus dismissal, member ordering, excluded/removed membership, role precedence,
and independent summary/member/coverage reads. Public list/detail operations describe pagination,
errors, and why an inspection view cannot authorize a later mutation without fresh validation. The
summary representative field documents its active-member fallback contract.

The documentation guide now records the recurring rule: describe read-snapshot consistency,
selection/order policy, and the distinction between diagnostic observations and mutation authority.
No query, output, or mutation behavior changes in this documentation revision. Member-decision
Clippy passes; combined workspace gates remain running before the next code extraction.

The final combined member-decision/inspection-contract tree passes all workspace tests/doctests, the
CLI build without default features, strict public/private Rustdoc, workspace Clippy, nightly
formatting, rumdl, and changed-page Markdown linting. Store-operation cleanup remains open; detail
member projection and remaining decision coordination are next.

## Continued maintenance: cluster member detail projection

`cluster_detail` now reads its summary, captures role coordinates, loads enriched members, and
returns the detail. `members` owns membership SQL, coverage acquisition, row decoding, and effective
role projection. `MemberRoles` names canonical archive-row identity and representative repository
number separately; `MemberProjection` shares those roles and loaded coverage across member rows.

The existing membership SQL, row ordering, read ordering, coverage behavior, error classifications,
and canonical/representative/related precedence remain unchanged. Read dependencies import actual
owners rather than the cluster-root prelude; unused root aliases are removed. Coverage helpers use
the existing crate-internal read boundary, whose visibility remains part of the workspace seam
audit. Three nearby linear cases cover role precedence, number-versus-row identity, and unrelated
members. Both cluster integration cases and initial workspace Clippy pass. Final workspace
validation is running; remaining decision coordination stays in the bounded store-operation batch.

## Continued maintenance: cluster mutation contracts

Decision documentation now distinguishes local dismissal from generation lifecycle and local
inclusion from canonical selection. It explains shared transactional state/event commits, repeated
valid audit actions, reason byte limits and trimming, current-member validation, and the distinction
between absent source threads and invalid cluster membership. Public operations describe their
specific effects without duplicating the shared transaction contract at each method.

The documentation is grounded in the existing SQL, member resolver, fence checks, and statement
order. No mutation or error behavior changes. Final member projection validation is running;
remaining cluster-level and canonical write coordination stays in the store-operation batch.

Final member-projection gates pass: all workspace tests/doctests including the three role cases, the
CLI build without default features, strict public/private Rustdoc, workspace Clippy, formatting,
rumdl, and changed-page Markdown linting. Store-operation review remains open.

## Continued maintenance: canonical selection phases

`CanonicalSelection` retains the resolved cluster/member and action time through active-membership
validation, canonical update, and audit insertion. The archive method remains the owner of checked
ID conversion, transaction/fence, current-member resolution, and commit. Each mutation phase now
fits locally and carries its own effect/failure contract.

Selection SQL is unchanged. Excluded membership rejection, missing-cluster checks, bind order,
canonical event label, generated-representative preservation, and commit timing retain their prior
behavior. Existing cluster integration scenarios cover successful local selection, durable
decisions, and nonmember rejection. Workspace Clippy passes; focused scenarios and final gates are
running. Remaining cluster-level dismissal coordination stays in the bounded store-operation batch.

Canonical selection's final gates pass: all workspace tests/doctests, the CLI build without default
features, strict public/private Rustdoc, workspace Clippy, formatting, rumdl, and changed-page
Markdown linting.

## Continued maintenance: cluster dismissal phases

`ClusterDecisionWrite` holds checked cluster identity, typed choice, validated reason, and action
time through named dismissal/restoration mutations and audit insertion. The archive coordinator
retains validation-before-writer precedence, transaction/fence, and one final commit. Dispatch
points directly to named mutations, and exactly-one-row validation precedes the event write.

SQL statements, bind order, reason trimming, event labels, missing-cluster behavior, lifecycle
preservation, and commit order are unchanged. The store batch's selected surfaces now expose
coherent read projections or transaction phases with effect/error contracts and recorded linear
SQL/policy exceptions. Final dismissal and store acceptance validation is in progress.

Dismissal's workspace Clippy and both existing cluster integration cases pass. Formatting, rumdl,
and changed-page Markdown linting pass. Store acceptance workspace gates are running. Existing store
integration cases exercise dismissal but not restoration; the bounded test-review batch must close
that specific coverage gap rather than treat SQL preservation as complete behavior coverage.

## Continued maintenance: public restoration coverage

A dedicated linear public-archive scenario dismisses and restores one generated cluster. It checks
normalized dismissal reason, cleared restoration state, retained lifecycle/title/representative, and
unchanged member role/state/coverage. Read-only audit inspection verifies ordered dismissal and
restoration events with trimmed and empty reasons respectively. Static fixtures construct domain
values only; all archive transitions remain visible in the scenario.

The new scenario and workspace Clippy pass. The prior store acceptance tree passes all workspace
checks; final acceptance reruns with this scenario included. The specifically recorded restoration
coverage gap is closed. Broader fixture, suite locality, and convention review remain in their
bounded batches rather than being silently removed from scope.

Store acceptance passes with direct restoration coverage included: all workspace tests/doctests, the
CLI build without default features, strict public/private Rustdoc, workspace Clippy, nightly
formatting, rumdl, and changed-page Markdown linting. The first five bounded batches are
implemented. Presentation, workspace conventions/documentation, test-suite review, and final
acceptance remain.

## Continued maintenance: named CLI timeline wording

CLI detail section assembly remains in `reports::detail`; event wording now lives in a private
`reports::timeline` module. Dispatch calls named opened/closed/comment/review projections. Borrowed
review-thread path, resolution, and outdated facts share one context for thread and comment wording,
without behavioral boolean parameters. Reviewer, state, and body suffix have explanatory locals.

Labels, punctuation, source-body spelling, missing-data fallback, timestamps, section order, and
JSON DTOs remain unchanged. Two nearby linear cases protect resolved/outdated and unknown-path
wording. Both focused cases, all offline query output cases, workspace Clippy, formatting, rumdl,
and changed-page Markdown linting pass. All workspace tests/doctests, the CLI build without default
features, and strict public/private Rustdoc also pass. Review-thread match arms retain only explicit
source-field projection into a named context; all wording policy lives below the dispatcher.
Presentation cleanup remains open.

## Continued maintenance: archive status presentation

`ArchiveStatusOutput::summary` coordinates named counts, coverage, work, lease, and schema methods.
The command uses that method directly. Lease owner and expiry, migration-history suffix, and work
counts have local names; no new wrapper or behavioral boolean parameter is introduced. Existing
wording, section order, fallback labels, coverage iteration order, and JSON projection are retained.

A linear offline command scenario protects human section order, counts, thread coverage, available
lease, and valid schema history. All three offline query scenarios pass. Workspace Clippy passed for
the implementation before the added scenario; final workspace validation remains to run.
Presentation and the subsequent workspace review batches remain open.

## Continued maintenance: TUI coverage presentation

A borrowed `CoverageView` presents identity, evidence, work diagnostics, and lease sections through
named methods. The draw function retains initial loading, error precedence, cached-refresh marker,
wrapping, and frame placement. Family order, heading emphasis, wording, owner/expiry fallback, and
held-only owner display remain unchanged. The context borrows one loaded projection and performs no
diagnostic refresh or archive operation.

A nearby linear on-disk archive scenario checks the health heading, empty work counters, and
available-lease line. All 68 TUI cases and its crate doctest pass. Workspace Clippy passes. The full
workspace acceptance gates remain to rerun for this change. Presentation review remains open,
including cluster detail and remaining list/detail summaries.

## Continued maintenance: cluster display projections

TUI cluster list wording belongs to `cluster_item`. A borrowed `ClusterDetailView` holds the loaded
member projection and selected index together. Its heading and member methods preserve repository
scope, counts, dismissal suffix, member order, optional three-decimal score, role/inclusion labels,
and matching selection marker/highlight. Drawing retains loading/error precedence, refresh markers,
selection eligibility, wrapping, and frame placement. Domain label matches remain exhaustive value
maps rather than behavioral dispatch.

The initial projection change passes all 68 TUI tests, its doctest, and workspace Clippy. A final
member-method simplification introduces named role/inclusion label queries and computes selection
once. Focused and full workspace validation pass for that final tree, including the previous
coverage presentation change: all tests/doctests, workspace Clippy, the CLI build without default
features, strict public/private Rustdoc, nightly formatting, rumdl, and changed-page Markdown
linting. Remaining CLI list/detail and TUI input/event-loop review stays open.

## Continued maintenance: terminal event-loop owner

A private `event_loop` module owns the live app, completion channel, and shared execution resources.
`EventLoop::run` coordinates initial dispatch, message draining, drawing, and input polling. Named
methods apply completion messages and dispatch operation refreshes, route accepted key events, and
start actions with one locally retained resource set. The launcher retains terminal setup and
restoration, awaited task shutdown, and archive closure. Channel capacity, message/draw/input order,
40 ms poll interval, accepted key kinds, and cancellation semantics remain unchanged.

The constructor's four parameters are explicit session execution resources, not an unexplained state
tuple. Dispatch accepts the initial fixed action array and later action vectors directly. Focused
TUI tests and workspace Clippy are running for this change; final workspace gates remain. Search
editing and browser input were reviewed and retained: their existing named actions separate
draft/apply/cancel transitions, pane navigation, selection invalidation, and query scope.

## Continued maintenance: embedding output ownership

`EmbeddingOutput::summary` presents its existing stage/report projection directly; the command uses
that method as its rendering callback. `representative_failure` documents and retains stage-first,
batch-second, document-third selection. The success branch consumes the formatted summary without an
unnecessary clone. Structured output, failure collections, wording, and exit policy are unchanged.

Focused embedding-output cases and workspace Clippy are running. Human failure-priority coverage and
final workspace gates remain to verify this change. The presentation inventory is now explicit in
the source-shape audit; the subsequent conventions/documentation and test-review batches remain.

## Continued maintenance: named TUI timeline wording

Current-evidence event wording now belongs to private `view::timeline`, while detail assembly and
scroll bounds remain in `view::detail`. Dispatch names creation, closure, comment, review, review
thread, and review comment projections. Review-thread source path/resolution/outdated facts share
one borrowed view; no behavioral boolean parameter is added. Named locals expose missing author,
reviewer, review body, and path fallback policy.

Wording, event order, body spelling, and review-comment omission of resolution remain unchanged. Two
nearby linear cases protect resolved/outdated source-path and unresolved/current missing-path lines.
Focused TUI tests and workspace Clippy are running. Full acceptance remains to run for the
embedding-output and TUI timeline changes. CLI page, cluster, and run-detail projections are the
remaining implementation targets in presentation cleanup.

## Continued maintenance: CLI page presentation

`reports::pages` gives existing thread/search output DTOs owned human summaries and result rows.
Shared footer formatting preserves scope-wide coverage counts and optional continuation. Search
policy/fallback stays with the page, row score/rank stays with the hit, and source kind/state labels
remain shared queries. The report adapter uses the summary method directly. No parallel DTO model,
JSON change, or query behavior is introduced.

All three offline query scenarios and workspace Clippy pass for the implementation. Two nearby
linear output cases protect empty-page wording and complete/incomplete/missing footer counts before
continuation. Both focused cases and final-tree workspace Clippy pass, along with nightly
formatting, rumdl, and changed-page Markdown linting. Full acceptance remains. Presentation's
remaining implementation inventory is CLI cluster and run-detail projections.

## Continued maintenance: CLI cluster projections

Cluster page/detail coordinators delegate named list-row, detail-heading, and member-row
projections. Lifecycle, role, and inclusion remain exhaustive label queries, keeping generation
lifecycle separate from dismissal and member role separate from inclusion. Empty-page wording,
pagination, dismissal reason, representative fallback, member totals, source title, and row order
remain unchanged.

The offline cluster contract and workspace Clippy pass. A nearby empty-page output case supplements
the existing decision JSON case; focused cases pass. Full acceptance will include this change with
CLI page and final run-detail presentation changes. A first attempted test target name was absent;
validation uses the repository's actual `cluster_contract` target.

## Continued maintenance: run detail presentation

A dedicated `reports::run_detail` module presents attempt identity, ordered jobs, and ordered
failures through named projections. The coordinator keeps headings and explicit empty-failure
wording visible. Explanatory start/parent/family/resolution locals preserve timestamp fallback,
parent dash, unassigned family, and resolved suffix. Run-list and retry summaries retain their
existing owning module and shared status-label queries. The command imports the detail renderer from
its actual owner.

All 14 CLI process contract cases and workspace Clippy pass for the implementation. A nearby exact
empty-run case checks identity, timestamp, parent, counts, both headings, and the no-failures line.
The focused empty-run case and all final workspace gates pass: Clippy, tests/doctests, the CLI build
without default features, strict public/private Rustdoc, nightly formatting, rumdl, and changed-page
Markdown linting. The sixth implementation batch is complete. Broader documentation-depth and
test-suite review remain separate bounded batches.

## Continued maintenance: core coverage and extension contracts

The conventions pass starts with a fresh 204-module production inventory. Function comment presence
is complete, but documentation content and public/private boundaries remain unproven. Forty-eight
introductions under ten lines and retained restricted visibility/preludes are explicit review
targets, not a mechanical length or visibility conversion task.

`Coverage::mark_stale` replaces the boolean setter with a named operation. Store projection
preserves fresh construction unless its existing freshness query requires stale marking. Existing
complete/stale coverage and document scenarios use the named operation. No serialized coverage field
changes. `ProviderData` explains object-only construction, unchanged rejected values, replacement
versus absent or null fields, and deterministic top-level access. A compiled example shows
successful object parsing and array rejection. Focused core tests/doctests and workspace Clippy are
running; broad review and full acceptance remain.

## Continued maintenance: acquisition and value-boundary documentation

Core observation fields explain family/payload, provider revision, local time, reserved sequence,
and completeness independently. Constructor documentation states that generic payload contents and
received counts are not validated and that archive allocation/application happen elsewhere.
`SourceClock::from_raw` documents trimming, missing versus invalid values, and preserved invalid
spelling, with a compiled example. Duplicate parse-error arms collapse to one unchanged
invalid-clock projection.

Timestamp docs explain signed microseconds, pre-epoch values, normalized UTC, precision loss, and
reconstruction/formatting errors. Vector docs explain exact dimension/byte correspondence, portable
encoding without a header, numeric revalidation, and separately owned model compatibility. Private
representation fields gain their own contracts. Focused core tests/doctests and workspace Clippy
pass. These targeted reviews do not complete the remaining core modules or workspace item audit.

Retained signature: `Observation::new` takes six explicit facts that directly construct its domain
envelope. A second parameter bag would add a concept without eliminating a validation or lifecycle
obligation; the constructor's newly documented limits keep those independent facts visible.

Document constructor/hash-query docs distinguish supplied rendering from derived validation and
spell out excluded deduplication/source-clock fields. Content's module guidance explains which
record relationships public construction/deserialization does not establish. These common limits
live at the module or constructor level rather than being repeated on every source field.

## Continued maintenance: checked identity contracts

Core identity constructors explain checked shape versus provider/archive existence, sequence
allocation, SQL range, and parent relationship validation. Private fields document their scope.
Thread/child docs identify which supplied parts participate in equality and hashing.
Thread-reference orientation corrects the misleading rename explanation: a repository display path
can change while its provider identity remains stable, and a local number still needs repository
scope.

Provider/commit docs explain preserved opaque spelling versus normalized full hexadecimal revisions.
Run IDs include a positive/zero example. Host parsing documents trimming, authority normalization,
default-port omission, ASCII DNS input, rejected URL forms, and its distinction from transport
request authorization, with a compiled example. Outcome guidance keeps workflow-specific count units
and accounting responsibility explicit. The reusable constructor-boundary rule is recorded in the
documentation guide. Focused core tests/doctests, strict core Rustdoc, workspace Clippy, formatting,
rumdl, and changed-page Markdown linting pass. Full workspace conventions and test reviews remain
open.

## Continued maintenance: GitHub transport dependency locality

Transport client, request, response, retry, and pagination files import HTTP, runtime, Serde, error,
and credential names from their actual owners. Shared transport policy/types remain explicit imports
from their owning module; the parent drops dependency imports that existed only as a child prelude.
No request, retry, origin, pagination, or credential behavior changes.

All 20 GitHub cases, two doctests, and workspace Clippy pass. Request-context and client fields
explain retained destination/body/cancellation and shared pool/permit ownership. Client methods
explain construction without requests or credential discovery, origin checking, one-page versus
pagination semantics, cancellation, bounded bodies, and typed failures. Strict GitHub Rustdoc,
nightly formatting, rumdl, and changed-page Markdown linting also pass. Remaining provider modules,
item depth, private protocol ownership, and restricted-visibility review remain in the conventions
batch.

## Continued maintenance: provider normalization dependency locality

REST acquisition and REST/GraphQL normalizers name their domain, transport, error, cancellation, and
JSON owners directly. Resource parents drop imported names used only by children. Provider wire
shapes remain distinct from normalized content and public page results. All 20 GitHub tests, both
doctests, and workspace Clippy pass. REST wire-type placement and item contracts are the next local
review surface; this slice does not complete the crate-wide review.

## Continued maintenance: REST response ownership

Private `resources::wire` owns raw REST repository/discussion/comment/pull-request/review response
shapes and nested user/label/branch values. Type and field contracts explain source spelling,
nullable fields, comment-body default, extension retention, and normalization obligations. Sibling
fetch and normalization code import those DTOs directly. Public page results stay in the 69-line
resource module; the wire leaf is 176 lines with documentation. Wire types/fields use `pub` inside
the private module and are not exposed as domain, archive, or CLI APIs.

Serde names, defaults, flattening, serialization derives, and normalized behavior are unchanged. All
20 GitHub tests, both doctests, workspace Clippy, strict GitHub Rustdoc, formatting, rumdl, and
changed-page Markdown linting pass. Full workspace gates remain to run. GraphQL response ownership
and remaining provider API contracts still belong to the bounded conventions review.

## Continued maintenance: GraphQL response ownership

Private `review_threads::wire` owns query variables, operation envelopes, outer/nested connections,
page metadata, and raw thread/comment/author/review nodes. Type and field contracts explain nullable
provider omission, required acquisition checks, independent nested pagination, source facts, and
extension retention. Normalization imports raw nodes from their actual private owner. The public
review-thread page and acquisition coordination remain separate from wire representation.

Serde shape, required/optional values, defaults, and normalized behavior are preserved. All 20
GitHub tests and both doctests pass. Clippy identified leftover imports after extraction; they are
removed. Final-tree workspace Clippy and strict GitHub Rustdoc pass, as do formatting, rumdl, and
changed-page Markdown linting. Provider acquisition/API contracts and broader convention review
remain open; extraction alone does not close the crate audit.

## Continued maintenance: REST URL and acquisition contracts

Initial REST URL construction belongs to `resources::urls`; public URL builders remain available
through the resource module. The URL leaf explains encoded display paths, source-state/update query
scope, 100-item pages, and separation from provider continuation. Fetch operations retain their own
normalization and cancellation story rather than mixing endpoint construction with acquisition.

Repository fetching documents caller responsibility for pairing a domain host with its configured
client. Scoped enumeration explains that a supplied continuation is used unchanged and must remain
paired with its original scan; origin validation alone does not prove resource scope. Page success
normalizes every item but establishes no durable complete-membership authority. Comment, metadata,
and review docs explain their specific identity and source-data limits. The internal scope guard is
named `require_thread_scope`, accurately describing repository equality rather than claiming to
validate pull-request kind. All endpoint and acquisition behavior is preserved.

The initial split passes all 20 GitHub tests, both doctests, and workspace Clippy. Final-tree
focused checks, Clippy, and strict GitHub Rustdoc are running after the additional API contracts and
guard rename. Broader provider and workspace convention reviews remain open.
