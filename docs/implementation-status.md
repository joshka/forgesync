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

Final-tree checks pass all 20 GitHub tests, both doctests, workspace Clippy, strict GitHub Rustdoc,
formatting, and Markdown checks. Broader provider and workspace convention reviews remain open.

## Continued maintenance: GraphQL request owner

Private `review_threads::request` owns query text and the borrowed `GraphqlRequest` pair of
operation and typed variables. Its `execute` method encodes the unchanged two-field request shape,
uses the existing GraphQL endpoint/transport/cancellation path, and rejects provider error envelopes
while retaining only the safe error count. Acquisition still validates required data and paginates
outer and nested connections; request execution proves no resource completeness and writes no
archive.

Final-tree checks pass all 21 GitHub tests, both doctests, workspace Clippy, strict GitHub Rustdoc,
formatting, and Markdown checks. A nearby linear encoding case checks named operation variables and
a null initial cursor. Nested pagination is addressed by the following change.

## Continued maintenance: nested comment pagination owner

Private `review_threads::comments::CommentPages` owns raw members, current continuation metadata,
consumed cursors, and the selected provider node. Consuming completion dispatches named cursor,
request, and page-application operations before exposing the completed node list. The review-thread
coordinator checks identity and initial connection, awaits completion, then normalizes the same
source/head context. No partial node list escapes on failure.

Request order, member order, cursor spelling, repeated-cursor rejection, error classification,
normalization, and cancellation behavior remain unchanged. Page application checks required members
and metadata before updating retained state. Three nearby linear cases cover repeated continuation,
terminal metadata without a cursor, and missing pagination flag. Final-tree checks pass all 24
GitHub tests, both doctests, workspace Clippy, strict GitHub Rustdoc, formatting, and Markdown
checks. Broader conventions/documentation and test-suite reviews remain open.

## Continued maintenance: review-page acquisition contracts

The public page and cursor contracts explain resource pairing, display-path versus stable identity,
caller-selected head evidence, whole-page failure, and the distinction between a terminal provider
page and durable replacement authority. The provider query does not verify the requested head or
thread kind; those obligations remain explicit at the workflow boundary.

The six acquisition inputs are retained: transport, repository routing, thread scope, evidence head,
continuation, and cancellation have distinct roles. A parameter bag would only rename those facts.
The outer coordinator retains its linear traversal and required-data checks; nested pagination state
has its own owner. Strict GitHub documentation, nightly formatting, repository Markdown linting, and
changed-file Markdown checks pass for this documentation change.

## Continued maintenance: checkpoint and health contracts

Store checkpoint documentation explains absent versus zero watermarks, independent source and
publication clocks, sequence-fenced advancement, completed-scan requirements, and transaction
rollback. The scan check proves completion of the named sequence, not the provider query's semantic
identity. The explicit arguments retain their distinct lease, repository, scan, and clock roles;
linear SQL binding and transaction order remain visible.

Health documentation explains report assembly, stable check order, failed probes versus diagnostic
errors, and connection-local temporary probes. Durable data remains unchanged, but the operation
executes temporary DDL. Health summarizes capability checks rather than pending work, evidence
completeness, or write authority. Separate pooled reads are not a single snapshot. Strict store
Rustdoc and nightly formatting pass. This is a contract clarification with unchanged behavior;
workspace acceptance and the remaining module/test reviews are still open.

## Continued maintenance: archive lease timing and ownership

Lease contracts now distinguish acquisition identity from current write authority, explicit clocks
from mutation-time clock checks, heartbeat expiry replacement from extension of the previous expiry,
and release of an expired owner from rejection of a successor. Token cloning and dropping have no
database effect; workflows own heartbeat and release. The raw-connection guard retains crate
visibility because exposing SQL resources would weaken the archive API boundary.

The conversion review found that a nonzero duration below one microsecond previously truncated to
zero and could create an immediately expired lease. Acquisition and heartbeat now reject that value
before a write. Four nearby linear cases cover zero, submicrosecond, precision truncation, and
signed-range overflow. All 50 store tests and both doctests, workspace Clippy, strict store
documentation, nightly formatting, and Markdown checks pass. Broader convention and test-suite
reviews remain open.

## Continued maintenance: keyword interpretation and result contracts

Keyword expression construction now directly splits on non-term characters, retains nonempty terms,
and quotes them in order rather than maintaining an imperative character buffer. The original
operator-looking scenario moves beside the implementation and becomes independent operator,
punctuation-only, Unicode/underscore, and empty-input scenarios. The FTS interpretation is
unchanged.

Keyword module contracts explain prefix acquisition, page-size bounds, rank provenance, coverage
selection, defensive continuation termination, and separate-read consistency. Ranking documentation
clarifies that its paging projection preserves an already ordered prefix rather than sorting, and
that fallback eligibility still requires the caller's preference. The remaining long candidate
collector and broad projection signatures remain review targets; documentation does not close them.
All 12 focused search tests, workspace Clippy, strict engine documentation, nightly formatting, and
Markdown checks pass.

## Continued maintenance: keyword candidate accumulation owner

`search::keyword::KeywordCandidates` now owns its accumulated prefix and coverage where its behavior
is implemented. The coordinator reads as request construction, local acquisition, page application,
and continuation. Page application retains successful members before rejecting empty or nonadvancing
continuation; first nonempty coverage and result order are unchanged. The request helper preserves
filters while replacing keyword mode and prefix coordinates.

One shared rank projection now serves both candidate collection and visible keyword pages, keeping
identical saturation and provenance rules local. The ranked coordinator imports the owner directly
from its module. No new parameter bag, public crate-root facade, or retrieval framework is added.
All 12 focused search tests, workspace Clippy, strict engine documentation, and nightly formatting
pass. All 89 engine tests and three doctests pass; broad keyword page projection signatures remain
under review.

## Continued maintenance: keyword projections use the owning request

Both keyword page projections now take the existing `SearchRequest` instead of seven separately
extracted query, mode, sort, and page facts. Request interpretation stays local: trimmed query,
default sort, provenance offset, and requested mode all come from that owner. A retained failure
reason selects effective keyword mode; ordinary keyword/advanced-FTS pages retain their mode.

The fallback coordinator still validates page coordinates before either branch, preserving error
precedence. Reused candidates and freshly fetched fallback pages preserve their existing ordering,
coverage, continuation, and safe failure reason. No new parameter-bag type is introduced. Full
engine tests (89 cases and three doctests), workspace Clippy, strict engine documentation, nightly
formatting, and Markdown checks pass.

## Continued maintenance: ranked projection input ownership

The prepared ranking input moves from the search root to its private `ranking` owner. Its type
contract distinguishes acquired, ordered projection facts from the original user request; each field
now documents validation, interpretation, mode/provenance, or coverage expectations. Keyword,
semantic, and hybrid callers import that input directly from its consuming module rather than the
parent as a dependency prelude. The shape and projection behavior remain unchanged.

A refreshed syntax inventory records 210 production module files and 44 introductions under ten
lines, with no missing handwritten production function comments under its exclusions. These remain
review signals, not evidence that all documentation is complete. The bounded audit records the
updated inventory and reviewed areas. All 12 focused search tests, workspace Clippy, strict engine
documentation, nightly formatting, and Markdown checks pass.

## Continued maintenance: semantic source and result contracts

Semantic module orientation now describes the retained source scope, first-page reuse, separate-read
consistency, query-vector transmission, bounded ranking evidence, and process-wide worker permits.
Source and ranking fields explain their roles. Method contracts distinguish archive filtering from
the dimension-only predicate, stored nonempty-chunk guarantees from standalone construction, and
worker cancellation from immediate task termination. Cosine evidence is not a probability.

The semantic page projection uses the existing `SearchRequest` instead of seven extracted values.
Its caller's validated window retains the same raw coordinates, so query trimming, sort defaults,
mode, ranks, page slicing, and coverage remain unchanged. All 89 engine tests and three doctests,
workspace Clippy, strict engine documentation, nightly formatting, and Markdown checks pass.
Remaining workspace conventions and test review stay open.

## Continued maintenance: configuration boundaries

CLI configuration documentation now explains explicit-path/environment/default precedence, missing
selected-file failure, defaulted versus unknown TOML fields, and deferred service validation. It
corrects the module's inaccurate claim to own archive settings and describes document recipe,
credential handoff, byte budgets, URL constraints, and client construction at their actual owners.
Validation proves shape and bounds, not connectivity, provider capability, or credential acceptance.

The focused configuration suite moves to the nearby `config/tests.rs` leaf with its own orientation
and explicit owner imports. Unsafe URL and undersized batch checks become independent scenarios.
Behavior and setting defaults remain unchanged. All eight focused configuration-related CLI tests,
workspace Clippy, strict CLI documentation, nightly formatting, and Markdown checks pass. Broader
workspace conventions and test review remain open.

## Continued maintenance: credential resolution owner

GitHub credential discovery becomes `GitHubCredentialSettings::resolve_token`; the settings own the
lookup policy and the caller supplies only host and cancellation. Provider setup constructs one
settings value and invokes the method. Module and method contracts now explain environment
precedence, empty versus malformed values, host scope, subprocess-only timeout/cancellation, and
safe typed failures. Anonymous fallback remains explicit at the command boundary.

The credential suite moves to a nearby leaf with direct owner imports and an explanation of its
synthetic values and Unix subprocess probes. Lookup behavior is unchanged. All five focused
credential tests, workspace Clippy, strict CLI documentation, nightly formatting, and Markdown
checks pass. Broader review remains open.

## Continued maintenance: terminal input and frame contracts

Input module orientation now explains key precedence, draft versus applied search, action submission
versus asynchronous completion, applied repository scope, and writer cancellation before quit.
Browser orientation explains pane-specific movement, explicit page requests, stable scope selection,
and stale detail invalidation. Triage orientation corrects the claim to apply completed operations:
it submits typed intent while reply handlers apply results, and store/engine enforce write
authority.

The view root explains header/body/footer composition, specialized leaves, presentation-state
mutation, and separation from query dispatch and durable decisions. Movement variants and nearby
triage test orientation gain specific contracts. Behavior remains unchanged. Strict TUI Rustdoc and
nightly formatting pass; broader module and test review remains open.

## Continued maintenance: local query and coverage contracts

Read-query orientation now explains ordered offset pages versus frozen snapshots, separate aggregate
and item coverage reads, sentinel removal, blank versus advanced FTS interpretation, shared SQL
aliases, and display lookup versus durable identity. Method contracts describe limits, errors, and
read consistency. The shared filter helper's inaccurate claim to add date predicates is corrected.

Coverage orientation describes current and acquired head facts, fixed applicable-family expansion,
missing rows, and stale inspection without rewriting complete membership. Query imports now name the
coverage helper owner directly. Behavior is unchanged. Store Clippy, strict store Rustdoc, nightly
formatting, and Markdown checks pass. Broader module/test review remains open.

## Continued maintenance: store failure contracts

Store error orientation explains lifecycle recovery, invalid input versus persisted facts, stale
writer/generation authority, wrapped source chains, and classification versus retry policy. Variant
docs correct narrow creation-time, sync-only lease, scan-status-only, and coverage-only JSON claims
to match their actual use. Duration failure includes the microsecond storage boundary.

The exhaustive error-code match remains a visible pure value map: extracting one function per
constant would make classification harder to review. Type and code contracts avoid promising atomic
rollback for every multi-step caller and distinguish stable codes from human display text. Strict
store Rustdoc, nightly formatting, and Markdown checks pass. Behavior is unchanged; broader review
remains open.

## Continued maintenance: migration validation and reporting

Migration orientation explains the embedded immutable catalog, history read versus checksum
validation, caller-established baseline, explicit application, and result interval. The validator's
incorrect claim to reject dirty records is corrected: the preceding current-version read owns that
check. Opening and diagnostics documentation no longer imply that an old schema can always be
opened.

Failure contracts distinguish a successful operation report from a durable recovery log. Earlier
completed migrations can survive a later failure, so retries inspect history rather than reusing an
assumed baseline. Pool-level helpers retain crate visibility at the archive lifecycle boundary;
public report types remain usable. Strict store Rustdoc, nightly formatting, and Markdown checks
pass. Behavior is unchanged; broader review remains open.

## Continued maintenance: run ledger dependency locality

Run lifecycle, failure persistence, and read-query leaves now import external/core dependencies from
their defining modules instead of relying on a parent import prelude. Shared ledger types and
helpers retain explicit imports from their actual run-module owner. Parent-only transport, lease,
and observation imports are removed, while imports needed by public record fields remain.

Lifecycle orientation explains scope records, supplied counts, evidence authority, transaction
fencing, and completion rejection. No SQL, ordering, failure accounting, or public shape changes.
Store Clippy, strict store Rustdoc, nightly formatting, and Markdown checks pass. The broader
conventions and test review remain open.

## Continued maintenance: run history read contracts

Run query orientation explains newest-first history, stable identity ties, ordered child records,
resolved failure inclusion, current repository payloads versus recorded scope/outcome, and separate
read consistency. Method contracts distinguish absence from decoding failure and state limit/range
requirements. Malformed persisted records reject projection rather than disappearing from results.

The ledger remains diagnostic state rather than a snapshot or write authority. Strict store Rustdoc,
nightly formatting, and Markdown checks pass. SQL and behavior are unchanged; broader review remains
open.

## Continued maintenance: failure ledger mutation contracts

Failure-ledger orientation and methods now explain optional resolved identity, exact selector versus
child-family scope, current-run exclusions, zero affected rows, retained history, repeated retry
increments, and resolution provenance. The resolution docs correct an implied store completion
check: engine callers establish successful matching work; store checks scope and lease authority.
Recording failure does not complete its job or mutate acquired evidence.

Strict store Rustdoc, nightly formatting, and Markdown checks pass. SQL and behavior are unchanged;
broader conventions and test review remain open.

## Continued maintenance: repository registration contracts

Repository storage orientation now distinguishes stable host/provider identity from replaceable
descriptive fields and the returned SQLite row key. It explicitly documents unconditional payload
replacement without discussion-style acquisition-sequence/source-clock ordering. A lease proves
write authority rather than freshness of the caller-selected repository description.

Both fenced and unfenced public methods describe intended use, transaction behavior, and failures.
Registration creates no child-family completeness or provider acquisition. Strict store Rustdoc,
nightly formatting, and Markdown checks pass. SQL and behavior are unchanged; broader review remains
open.

## Continued maintenance: observation sequence allocation contracts

Sequence orientation now explains the archive-wide counter, committed allocation, valid gaps after
failed acquisition, diagnostic start timestamp, and separation from provider time, generation
reservation, and completeness. Both public methods document fenced/unfenced use and failures. The
private implementation's misleading always-fenced description is corrected; its guard is optional.

The checked conversion import names its actual observation-module owner. Strict store Rustdoc,
nightly formatting, and Markdown checks pass. SQL and behavior are unchanged; broader review remains
open.

## Continued maintenance: document rendering and persistence contracts

Document orientation now distinguishes pure recipe rendering, local detail reads, and leased
materialization. Contracts describe complete/nonstale child evidence, deterministic ordering, bot
omission, deduplication text, and hash ownership. Review helper descriptions no longer claim to
deduplicate sections when they only append ordered text.

Materialization documents the read-before-lease consistency limit: store validates supplied recipe
and hash but does not rerender or compare the earlier evidence snapshot. It also explains release
attempts, write-error precedence, and a possible release error after durable persistence. Strict
engine Rustdoc, nightly formatting, and Markdown checks pass. Behavior is unchanged; broader review
remains open.

## Continued maintenance: document recipe rendering owner

Private `documents::render` now owns pure recipe assembly, attribution, child ordering, filtering,
and deduplication text. The public document module retains local reads, leased persistence, report
construction, and its existing public rendering API. The split makes the workflow visible without
mixing it with every text-section helper. Tests retain nearby public recipe scenarios and explicit
owner imports. Rendered bytes and persistence behavior are unchanged.

All three focused document scenarios pass. Workspace Clippy and strict engine Rustdoc are running on
the completed split; broader conventions and test review remain open.

## Continued maintenance: CLI result projection contracts

Output orientation now explains envelope shape, structured partial success, inherited read
consistency, retained stale child evidence, search provenance, and serialization omissions.
Constructors accept supplied strings/data without redaction, validation, printing, or exit-code
selection; safe message responsibility is explicit. Search query documentation reflects engine
trimming, and child-family descriptions direct readers to coverage instead of implying freshness.

Nearby envelope tests gain orientation and an explicit owner import. All three focused output tests,
strict CLI Rustdoc, nightly formatting, and Markdown checks pass. Serialized shapes and behavior
remain unchanged; broader review remains open.

## Continued maintenance: selector identity contracts

Selector orientation now distinguishes checked display coordinates from resolved durable IDs,
case-sensitive equality from case-insensitive local lookup, and literal path grammar from a general
URL parser. Fields and construction/formatting contracts explain rename, existence, kind, and
percent-decoding limits. A runnable thread-selector example demonstrates the local number/path.

All four engine doctests, strict engine Rustdoc, nightly formatting, and Markdown checks pass.
Parsing behavior is unchanged; broader conventions and test review remain open.

## Continued maintenance: local selector parsing scenarios

The selector suite moves beside the parser in `reference/tests.rs` with orientation and explicit
owner imports. Combined success cases become independent default-host, enterprise, numbered-pair,
and pull-URL scenarios. Rejections now assert their exact typed classifications rather than only
`is_err`. New direct cases establish literal percent escapes and retained display-case equality.

All 12 focused reference-related tests, engine Clippy across targets/features, nightly formatting,
and Markdown checks pass. Parser behavior is unchanged; broader conventions and test review remain
open.

## Continued maintenance: embedding error policy contracts

Embedding error orientation now distinguishes preparation, attempts, response validation, safe
classification, bounded retry, and search fallback. The key-missing variant no longer incorrectly
claims the library reads environment. The restricted retry method is retained as an explicit API
exception: it governs the adapter loop, while the publicly re-exported error owns reporting.

Eight named linear rstest scenarios cover transient eligibility and nonretryable authorization,
cancellation, output, and redirect failures. All eight cases, strict engine Rustdoc, nightly
formatting, and Markdown checks pass. Retry behavior is unchanged; broader review remains open.

## Continued maintenance: embedding response validation owner

Decoded service response validation becomes a consuming method on `EmbeddingResponse`. Transport and
tests invoke the owner directly rather than importing a behavioral helper through the parent.
Envelope/item fields and module contracts explain optional model echo, exact indexes, accepted
reordering, request-order restoration, numeric conversion, and batch-wide dimensions. The former doc
claim to reject reordered vectors is corrected. No partial validated batch escapes failure.

All 16 focused embedding-client tests, engine Clippy across targets/features, strict engine Rustdoc,
nightly formatting, and Markdown checks pass. Request/response behavior remains unchanged; broader
review remains open.

## Continued maintenance: response validation scenario locality

Malformed embedding-response scenarios move to the nearby `response/tests.rs` leaf, leaving HTTP
request and redirect cases with the client. Fixtures now assert `InvalidResponse` for count/index
failures and `InvalidVector` for numeric/dimension failures instead of only checking `is_err`. The
test orientation explains its request-relative facts and direct wire decoding; imports name the
actual response and client owners rather than a parent dependency prelude.

All 16 focused embedding-client tests, engine Clippy across targets/features, nightly formatting,
and Markdown checks pass. Production behavior is unchanged; broader review remains open.

## Continued maintenance: refresh status accounting contracts

Refresh status orientation now explains selected-stage order, remaining-list preparation,
interruption precedence, primary failure retention, and safe typed display dependence. Outcome
counts are stage units; partial failure counts all remaining stages while deferred counts its
subset, so readers must not sum them as disjoint populations. Projection trusts coordinator-prepared
records and performs no acquisition, scheduling, or completeness validation.

Core outcome and engine error imports name their defining modules rather than a parent dependency
prelude. Strict engine Rustdoc, nightly formatting, and Markdown checks pass. Behavior is unchanged;
broader review remains open.

## Continued maintenance: refresh dependency locality

Refresh coordinator imports now name standard, core, provider, store, Tokio, and engine owners
directly. The parent retains dependencies used by its actual report/request types rather than
serving a child prelude. Embedding-stage sibling/status imports also name their defining modules.

All eight focused refresh tests, engine Clippy across targets/features, nightly formatting, and
Markdown checks pass. Workflow ordering and behavior are unchanged; broader review remains open.

### Local inspection command boundaries

Thread and run command orientations now describe request conversion, read-only archive lifetime,
coverage versus workflow-ledger meaning, and report ownership. Run documentation includes its retry
variant and identifies the acquisition boundary rather than describing every variant as a read. TUI
startup documentation records terminal prerequisites, writable archive access, credential setup, and
transfer of shutdown responsibility to the browser. Retry and GitHub setup imports name their
defining command modules. Strict CLI Rustdoc passes with private items included.

### Archive command lifecycle contracts

Archive command orientation now separates creation, explicit migration, status projections, and
health checks. Migration docs explain that a later reporting failure does not undo schema changes
and that earlier migrations may survive a later migration failure. Doctor docs distinguish an
unhealthy completed report from an error obtaining one, and scope health to the selected local
checks rather than source freshness. The command closes each handle before rendering; capability
probes use temporary storage rather than changing durable archive data.

Strict CLI Rustdoc with private items, nightly formatting, rumdl, and changed-page Markdown linting
pass for this documentation-only change.

### Cluster command orientation

Cluster command modules now distinguish generated proposals, stored-result inspection, and
maintainer-authored decisions. Their introductions explain argument types, engine/store validation,
archive access, cancellation setup, and report ownership. Decision docs describe acknowledgments
rather than implying that commands reload detail, and explain that output failure does not undo a
committed local action. Read docs scope persisted results to stored generation data rather than
claiming current source evidence or embeddings.

Strict CLI Rustdoc with private items, nightly formatting, rumdl, and changed-page Markdown linting
pass for this documentation-only change.

### CLI parsing and process boundaries

The binary introduction explains OS-string argument forwarding, runtime ownership, and where process
responsibilities belong. Argument-value orientation distinguishes local filters from provider
acquisition scope and corrects the description of search mode: semantic and hybrid queries can
request a query embedding while reading discussion evidence locally. Pure exhaustive value
conversions remain inline, with behavioral interpretation in command owners.

Strict CLI Rustdoc with private items, nightly formatting, rumdl, and changed-page Markdown linting
pass for this documentation-only change.

### Thread report presentation contracts

Thread report orientation now identifies DTO adaptation, shared output rendering, and the separate
page/search/detail layout owners. Display helper contracts explain retained unknown provider states,
stable repository identity versus owner/name selectors, timestamp fallback behavior, and coverage
labels that omit staleness and associated evidence. These functions format existing projections
rather than loading, filtering, or certifying source completeness.

Strict CLI Rustdoc with private items, nightly formatting, rumdl, and changed-page Markdown linting
pass for this documentation-only change.

### Browser and failure view orientation

TUI browser and failure views now explain wide/compact layouts, loaded-projection ownership,
selection versus applied scope, loading/error precedence, and retained-detail behavior. Browser docs
identify scroll clamping as presentation state and explicitly describe its line-based bound rather
than claiming wrapped-row measurement. Failure docs separate run projection from evidence coverage
and describe absence messages without implying retry targets or completed acquisition.

Strict TUI Rustdoc with private items, nightly formatting, rumdl, and changed-page Markdown linting
pass for this documentation-only change.

### Engine inspection API contracts

Inspection orientation now maps filters, requests, projections, and shared query adapters. Public
list/show contracts describe pagination validation order, missing repository versus discussion
errors, retained evidence, caller-owned archive lifetime, and separate-read consistency limits. The
shared sort field now correctly distinguishes ordinary listing's update default from search's
workflow-selected default. Internal resolution and pagination helpers document deduplication,
empty-scope meaning, and their restricted public API boundary.

Strict engine Rustdoc with private items, nightly formatting, rumdl, and changed-page Markdown
linting pass for this documentation-only change.

### Exact scoring contracts and test locality

Exact vector search now documents dimension rejection, positive best-chunk scoring, cancellation
checks, caller-owned model/freshness eligibility, and bounded page merging without deduplication.
Ordering comments correctly distinguish relevance scores from explicit source timestamp sorts.
Scored-thread fields have contracts. The larger inline test suite moves to a nearby file with
explicit defining-module imports and an orientation identifying the policies its scenarios cover.

All three focused exact-scoring tests pass. Strict engine Rustdoc initially caught a public module
link to an internal type; the reference now uses plain code text and the rerun passes. Nightly
formatting, rumdl, and changed-page Markdown linting also pass.

### Migration build dependency orientation

The store build script now explains Cargo directory/file dependencies, SQLx embedding ownership,
package-relative paths, and best-effort enumeration. Its documentation explicitly distinguishes
rerun directives from SQL validation, database generation, and runtime archive lifecycle. No build
script behavior changes.

Store compilation, nightly formatting, rumdl, and changed-page Markdown linting pass.

### Direct family coverage read contracts

Observation coverage documentation previously described writes despite containing only a read. The
module and method now describe recorded-state lookup, missing family versus missing discussion, JSON
decoding failures, and separate pooled reads. They explicitly identify that this method does not
derive timestamp/head staleness; richer read projections own that comparison. Shared helpers are
imported from their defining observation module. Behavior is unchanged.

Strict store Rustdoc with private items, nightly formatting, rumdl, and changed-page Markdown
linting pass.

### Family finalization contracts

Family finish orientation now explains declaration validation, head-bound review requirements,
reservation skip/replay outcomes, complete membership promotion, and incomplete coverage-only
application. It identifies the transaction owner, fenced versus unfenced authority, and separately
durable staging after finalization rollback. The convenience method documents why complete review
families need the context-aware form. Application imports now name their defining modules.

Strict store Rustdoc with private items, nightly formatting, rumdl, and changed-page Markdown
linting pass.

### Refresh execution orientation

Refresh coordinator and embedding-stage introductions now explain selection validation limits, fixed
stage order, independent attempts after failure, caller-owned services, and structured partial
outcomes. Embedding docs describe per-document isolation, repository page-read failure, continued
traversal after embedding failure, cancellation boundaries, separate-read pagination, and aggregate
counts that are not unique-identity or source-completeness claims.

Strict engine Rustdoc with private items, nightly formatting, rumdl, and changed-page Markdown
linting pass for this documentation-only change.

### Clustering candidate and decision boundaries

Candidate docs now identify the evidence/component/proposal pipeline, validated options and
duplicate identities, caller-owned eligibility, and edge-count scope before bounded grouping.
Decision docs explain read versus write authority, pre-lease selector resolution, store validation,
fixed leases, and operation-versus-release error precedence after durable writes. Private member
actions have item/variant docs and touched imports name their defining clustering modules.

Strict engine Rustdoc with private items, nightly formatting, rumdl, and changed-page Markdown
linting pass.

### Engine failure reporting orientation

Engine error docs now separate validation, local targets, service availability, cancellation, worker
failures, and wrapped owner errors. They explain partial durable progress despite errors,
classification versus retry policy, caller-owned presentation, and preservation of original provider
failure alongside a failed ledger write. Display/source diagnostics are not described as universally
safe public payloads.

Strict engine Rustdoc with private items, nightly formatting, rumdl, and changed-page Markdown
linting pass. The remaining conventions pass still includes API shape, visibility, imports, and
item-contract depth; introduction length alone does not close it.

### Family workflow import locality

Family application and staging imports now identify the defining family modules directly. The
observation root's module map also corrects its coverage entry to describe a recorded-state read,
matching the leaf contract corrected earlier. These changes preserve the existing domain and SQL
boundaries rather than exposing internal connections or converting private helpers to public APIs.

Store Clippy across all targets/features, strict private-item Rustdoc, nightly formatting, rumdl,
and changed-page Markdown linting pass.

### Observation application import locality

Parent observation application and SQL row conversion now import their dependencies through the
defining observation modules. The shared application helper also correctly documents optional lease
fencing rather than implying that the unfenced public entry point is fenced. No ordering, payload
mapping, or transaction behavior changes.

Store Clippy across all targets/features, nightly formatting, rumdl, and changed-page Markdown
linting pass.

### Linear exact-scoring numerical scenarios

The combined arithmetic test is split into named axis cases, a diagonal tolerance scenario, and a
dimension-mismatch scenario. Expected directions are explicit rstest inputs, and the diagonal test
names its difference before asserting it. A large-finite-component regression exercises the scaled
accumulation contract without computing expected results through a test helper.

All eight focused exact-search scenarios pass, together with nightly formatting, rumdl, and
changed-page Markdown linting.

### Accumulated cleanup workspace validation

The current six-crate tree passes all workspace tests and doctests with all features and the locked
dependency set. Workspace Clippy passes across all targets/features with warnings denied. The CLI
build without default features passes. Strict workspace Rustdoc includes private items and denies
warnings, missing public documentation, and broken links. Nightly formatting, repository rumdl, and
Markdown linting of the status/audit pages pass.

Logs are in `/tmp/forgesync-cleanup-workspace-tests.log`,
`/tmp/forgesync-cleanup-workspace-clippy.log`, `/tmp/forgesync-cleanup-cli-build.log`, and
`/tmp/forgesync-cleanup-workspace-doc.log`. This validates accumulated changes locally; the bounded
conventions review, broad test-quality review, and requirements reconciliation remain open. These
results do not establish hosted platform-matrix outcomes.

### Typed discussion filter arguments

Thread listing and search now flatten the same CLI-owned `ThreadFilterArgs` into their arguments.
The type owns conversion to engine filters, replacing a six-parameter bare function and the list
variant's construction block. Request conversion preserves absent sort policy for the workflow.
Repository, kind, state, limit, and offset flags keep their names and defaults; shared sort help now
serves both commands. The initial compile caught a missing module declaration, which is corrected.
All CLI tests and its doctest pass.

CLI Clippy across all targets/features, strict private-item Rustdoc, nightly formatting, rumdl, and
changed-page Markdown linting also pass.

### Shared filter parsing contract scenarios

Four nearby linear scenarios establish default scope/page values with an absent sort, explicit
repository/kind/state/order/window conversion, and rejection of zero and excessive limits. A small
parser flattens the real argument type; assertions inspect the resulting engine request rather than
using behavior helpers to calculate expectations. Shared sort help includes relevance. All four
focused tests pass after correcting the expected selector construction to its FromStr API.

### Private query adaptation boundary

Repository resolution, page validation, and engine-to-store state/sort mappings move from the public
inspection module into a private engine query module. Shared functions use ordinary public
visibility inside that private boundary, replacing four crate-restricted inspection helpers.
Inspection, search, and clustering import that owner directly. The public inspection vocabulary and
operation API remain under inspection; no SQL resources are exposed. Engine compilation passes.

All 88 engine unit scenarios pass. Engine Clippy across all targets/features, strict private-item
Rustdoc, nightly formatting, rumdl, and changed-page Markdown linting pass.

### Private scoring boundary

Public exact-search arithmetic is separated from internal ranked-candidate policy. The private
scoring module owns scored projections, best-chunk selection, bounded merging, and stable ordering;
its five formerly crate-restricted declarations use ordinary public visibility within that module.
Search and clustering import their defining owner directly. Cosine similarity retains its public
path and now has a focused arithmetic orientation. No scoring or ranking behavior changes.

All 88 engine unit scenarios, engine Clippy across all targets/features, strict private-item
Rustdoc, nightly formatting, rumdl, and changed-page Markdown linting pass.

### Scoring test locality

Cosine arithmetic scenarios retain only vector construction and direct numerical expectations.
Ranked-candidate scenarios and their discussion construction fixtures move beside the private
scoring owner. Both test modules explain what they establish and the timestamp-order policy they do
not cover. The split preserves every scenario while removing cross-owner setup from arithmetic.

All 88 engine unit scenarios, nightly formatting, rumdl, and changed-page Markdown linting pass.

### Shared engine wall-clock owner

Duplicated document/enumeration clock helpers move into a private engine clock module. Acquisition,
embedding, clustering, and lease workflows import that owner directly instead of depending on an
unrelated workflow. Documentation distinguishes wall time from monotonic timers and durable
observation ordering, including precision loss and clock-range errors. Both former restricted
helpers are removed. Initial Clippy caught an obsolete document timestamp import, now removed.

All 88 engine unit scenarios, engine Clippy across all targets/features, strict private-item
Rustdoc, nightly formatting, rumdl, and changed-page Markdown linting pass.

### Shared provider failure classification

Enumeration's provider-error conversion moves into a private provider-failure module. Parent and
child acquisition workflows import that owner directly. Ordinary public visibility replaces the
restricted helper while the private module preserves its internal status. Documentation explains
specific versus fallback categories, caller-owned cancellation interpretation, and typed diagnostic
messages without claiming additional redaction. The exhaustive value mapping is preserved.

Engine Clippy across all targets/features, strict private-item Rustdoc, nightly formatting, rumdl,
and changed-page Markdown linting pass.

### Provider classification contract scenarios

Ten named typed-input scenarios establish specific failure categories and protocol/cancellation
fallbacks. A separate linear scenario asserts the exact typed GraphQL diagnostic retained by the
adapter. The nearby suite explains that cancellation classification does not decide whether an
acquisition workflow should record a failure; that remains workflow policy.

All 11 classification scenarios pass; the filter also runs one existing provider-failure scenario
for 12 passing tests. Nightly formatting, rumdl, and changed-page Markdown linting pass.

### Engine visibility candidate disposition

The remaining engine restrictions are documented exceptions: enumeration's reserved-context/executor
bridge shared with sync, and retry policy on the public embedding error type. Enumeration comments
explain caller-owned reservation/fence obligations and the public coordinator alternative. The audit
records these retained seams and the private owners created for query, scoring, clock, and provider
failure policy. This closes engine visibility candidates without claiming the workspace pass done.

Strict engine private-item Rustdoc, nightly formatting, rumdl, and changed-page Markdown linting
pass.

### Observation SQL visibility boundary

Observation clock columns, checked SQL integers, canonical identity lookup, and coverage persistence
move into a private observation-SQL module. Public domain results remain in observations. Shared SQL
helpers use ordinary public visibility within the private boundary, and family/run/read callers
import that owner directly. Its orientation states caller-owned ordering, lease, and commit duties.
The first compile exposed an old qualified column type path and unused imports, now corrected.

All 26 store unit scenarios, store Clippy across all targets/features, strict private-item Rustdoc,
nightly formatting, rumdl, and changed-page Markdown linting pass. Integration scenarios still need
the next combined workspace run; unit checks alone do not prove observation ordering invariants.

### Private coverage projection owner

Coverage row decoding and freshness projection move to a private store module shared directly by
thread reads, embedding eligibility, and cluster members. Recorded clock/head fields and the family
catalog gain owner-level contracts. The public reads root no longer carries the internal row type or
re-exports coverage helpers. Its remaining intermediate thread row gains item/field docs. Initial
Clippy caught a now-unused root import, which is removed. Projection behavior is preserved.

All 26 store unit scenarios, store Clippy across all targets/features, strict private-item Rustdoc,
nightly formatting, rumdl, and changed-page Markdown linting pass.

### Shared SQL filter owner

Bound repository/discussion predicates move from the browsing query module into private query-SQL
adaptation. Browsing, aggregate coverage, and embedding eligibility name the owner directly; the
public reads root drops its final restricted helper re-export. The new orientation documents alias
and existing-predicate obligations, empty-scope meaning, and the policies left to each query owner.
The touched browsing query imports its own root-defined types explicitly.

Store Clippy across all targets/features, strict private-item Rustdoc, nightly formatting, rumdl,
and changed-page Markdown linting pass. Combined integration validation remains pending for the
recent store ownership moves.

### Store boundary integration validation and disposition

The complete store test suite and doctests pass after observation-SQL, coverage-projection, and
query-predicate ownership moves (`/tmp/forgesync-store-boundaries-tests.log`). Integration scenarios
exercise ordering/replay, family completeness, rollback, clustering decisions, and scan
finalization. The visibility audit retains raw pool capabilities, lifecycle/migration operations,
and lease checks as documented exceptions; archive pool fields and lifecycle helper contracts now
explain those boundaries directly. This closes store visibility candidates, not the full
conventions/test review.

### Exact terminal workflow target assertions

A combined sync/refresh test previously checked only that one repository was selected. Separate
linear scenarios now assert the complete emitted action with the expected repository selector. Retry
asserts its exact run action directly rather than using a conditional pattern guard. The suite
imports app types from their defining module. This strengthens target correctness rather than merely
restyling assertions.

All 11 focused app-transition scenarios, nightly formatting, rumdl, and changed-page Markdown
linting pass.

### Exact cluster-member action scenarios

The combined maintainer-key test ignored the emitted member field. Separate exclusion and canonical
scenarios now compare the entire action against an explicit discussion selector and cluster ID. Each
test has one keyboard action and one direct expected result, so wrong-member routing cannot pass
merely because the cluster ID or action variant is correct.

All 12 focused app-transition scenarios, nightly formatting, rumdl, and changed-page Markdown
linting pass.

### Independent dismissal and retry scenarios

The test that dismissed a cluster and then manually switched screens to retry a run is split into
independent cases. Each initializes only its relevant panel, performs one key action, and compares
the complete expected target. The tests no longer imply that retry depends on dismissal state or
require readers to carry unrelated panel fixtures through the scenario.

All 13 focused app-transition scenarios, nightly formatting, rumdl, and changed-page Markdown
linting pass.

### Complete browser query assertions

Search submission now asserts the submitted text, repository scope, and first-page offset in the
emitted action. Repository selection asserts the complete browsing action instead of destructuring
only some fields with a branch. A wrong search payload or retained query can no longer pass these
scenarios merely because one action was emitted.

All 13 focused app-transition scenarios pass. Nightly formatting, rumdl, and changed-page Markdown
linting pass.

### Complete repository scope transitions

Picker refresh tests compare the complete typed scope after insertion, empty results, failure, and
rename. The failure scenario previously checked only the first entry and could miss an accidental
additional repository. Direct owner imports and explicit expected selectors keep scope policy
visible without conditional assertions or behavior helpers.

All five focused repository-picker scenarios and nightly formatting pass. Rumdl normalization and
changed-page Markdown linting pass.

### Failure detail cutoff identity and ordering

The bounded failure-selection test now identifies the last retained and first omitted runs and
compares the complete result. A separate linear scenario places twenty completed records before an
unfinished record, proving filtering precedes the detail bound. These assertions detect reversed
selection or applying the bound before filtering, which a count-only test could miss. Repeated
records are explicit construction fixtures; no scenario loops or behavior helpers are needed.

All four focused failure-projection scenarios and nightly formatting pass. Rumdl and changed-page
Markdown linting pass.

### Fallback task cancellation coverage

The task-lifetime suite now exercises dropping the owner with a live writer. It checks both the
shared cancellation signal and a cleanup acknowledgment, so aborting the writer cannot masquerade as
cooperative shutdown. Explicit shutdown gains a bounded test wait to turn a lost cancellation signal
into an actionable failure rather than a hanging suite. Construction and cleanup are visible in each
linear scenario.

The complete TUI suite passes: 75 unit scenarios and one doctest. Nightly formatting, rumdl, and
changed-page Markdown linting pass.

### Provider pagination identity assertions

The enterprise thread-page scenario compares the complete resolved next URL, including origin and
base path, rather than checking only its query string. Nested review-comment pagination compares
both provider identities in order instead of only counting two records. These tests now establish
the routing and membership contracts stated in their names. Resource imports point to their owners.

The complete GitHub suite passes: 24 unit scenarios and two doctests. Nightly formatting, rumdl, and
changed-page Markdown linting pass.

### Provider fixture contracts and comment provenance

GitHub fixture helpers now explain their construction-only role and which fields intentionally stay
constant between nested pages. REST comment pagination checks each page's body, parent identity, and
provider identity, so second-page content cannot be silently paired with the wrong provenance.

The complete GitHub suite passes: 24 unit scenarios and two doctests. Nightly formatting, rumdl, and
changed-page Markdown linting pass.

### Explicit diagnostic ledger fixture

The read-only diagnostic integration scenario now inserts its failed-comments and deferred-reviews
jobs as explicit rows and attaches evidence to that run. The fixture no longer requires readers to
execute a loop mentally to recover the two intended cases. The workspace Clippy gate passes on the
preceding accumulated cleanup tree; this fixture change receives its own focused validation.

All six archive-lifecycle integration scenarios pass. Nightly formatting, rumdl, and changed-page
Markdown linting pass.

### Named cluster fixture selection and cleanup disposition

Cluster persistence scenarios use named active/all queries instead of a boolean helper argument. The
remaining fixed SQLite sidecar loops in the inspected store and engine integration fixtures serve
cleanup only and are retained with explicit contracts. Unique-path helpers now state that they do
not create archives. Linked test guidance records the distinction between scenario branching and
incidental resource cleanup.

Both cluster-persistence integration scenarios, nightly formatting, rumdl, and changed-page Markdown
linting pass. The other fixture edits are documentation-only.

### Observation fixture side-effect contracts

Observation integration helpers document which setup creates an archive, registers a repository,
reserves a durable sequence, or opens raw SQL for trigger/corruption scenarios. Value constructors
state which clocks and completeness facts remain controlled by the caller. Canonical-title reads
explain their independent committed-state check. These contracts expose the fixture assumptions
without moving assertion behavior into helpers.

All seven observation-transaction integration scenarios, nightly formatting, rumdl, and changed-page
Markdown linting pass.

### Accumulated workspace test validation

The complete locked, all-feature workspace test command passes on the accumulated cleanup tree,
including unit, integration, and documentation scenarios in all six crates. Evidence is recorded in
`/tmp/forgesync-current-workspace-tests.log`. The earlier workspace Clippy pass is recorded in
`/tmp/forgesync-current-workspace-clippy.log`; later fixture edits still need the final lint pass.
This establishes regression evidence, not completion of the outstanding catalog responsibility and
workspace documentation/API review.

### Separate fixture catalog contracts

Core catalog validation is split into named payload-hygiene, reference-integrity, and scenario
coverage tests. Each failure now identifies its responsibility. Exhaustive file/catalog loops remain
because the input set grows with the repository; fixed cases would miss newly added fixtures. Helper
contracts and the module introduction explain traversal and setup without hiding assertions.

All four fixture-catalog checks, nightly formatting, rumdl, and changed-page Markdown linting pass.

### Clustering fixture identity coherence

Engine clustering setup derives title numbering from the thread identity instead of accepting a
second independently supplied issue number. Document and observation helpers now cannot disagree
about that identity fact. Their contracts expose sequence reservation, complete observation writes,
fenced document/vector writes, fixed acquisition time, and the absence of service calls. Endpoint,
model, and vector values remain explicit scenario inputs rather than a hidden generic fixture bag.

The clustering-workflow integration scenario, nightly formatting, rumdl, and changed-page Markdown
linting pass.

### Remove redundant observation fixture identity

Observation setup returns the archive and thread identity without also exposing the repository
identity already contained in that thread. Discussion construction accepts the thread directly; it
no longer needs a redundant repository argument or an incidental assertion to keep both aligned.
This removes a live fact from each parent/child/rollback scenario while preserving explicit clocks,
sequence, payload, and completeness inputs.

All seven observation-transaction scenarios, nightly formatting, rumdl, and changed-page Markdown
linting pass.

### Direct observation test ownership imports

Parent, child, ordering, and rollback suites import domain values and store APIs from their defining
modules. The integration root no longer supplies a prelude of unrelated imported types. Explicit
parent imports remain only for fixture helpers genuinely defined there, making their setup ownership
visible without duplicating construction code across suites.

All seven observation-transaction scenarios, nightly formatting, rumdl, and changed-page Markdown
linting pass.

### Independent ordering regression scenarios

The combined observation-ordering test is split into source-time precedence, minimum legacy-sequence
conversion, and revision fallback without sequences. Each has one direct expectation and its own
contract name; a failure no longer requires identifying which unrelated assertion failed inside a
broad ordering scenario.

All nine observation-transaction scenarios, nightly formatting, rumdl, and changed-page Markdown
linting pass.

### Direct store read-test ownership imports

Detail, FTS, list/search, and migration integration suites import their domain, query, archive, and
SQL types directly. The parent module now owns shared fixture construction rather than acting as an
API prelude. Explicit parent imports identify only the setup helpers implemented there.

All four read/search integration scenarios, nightly formatting, rumdl, and changed-page Markdown
linting pass.

### Search fixture effect and target contracts

Search setup helpers document durable observation reservation/application and derived index effects.
The keyword page helper has a query-specific name and states its fixed page, scope, and ordering.
FTS update assertions check the selected discussion identity and replacement title in addition to
result counts, so a hit on the wrong discussion cannot satisfy the update scenario.

All four read/search integration scenarios, nightly formatting, rumdl, and changed-page Markdown
linting pass.

### Migration report and backfill identity assertions

The search migration scenario checks its baseline/final schema and exact ordered migration versions
instead of only the number applied. Its backfilled keyword result must match the original discussion
identity and title. This makes the intended schema transition and preserved searchable content
explicit in the test, with no scenario-dependent helper logic.

All four read/search integration scenarios, nightly formatting, rumdl, and changed-page Markdown
linting pass.

### Current strict workspace documentation gate

Strict all-feature workspace Rustdoc passes with private items included and warnings, missing public
documentation, and broken intra-doc links denied. Evidence is in
`/tmp/forgesync-current-workspace-doc.log`. This verifies compilable documentation and links, not
semantic depth for every contract. The cumulative audit now distinguishes earlier open findings from
later completed visibility dispositions and records the recent test review surfaces.

### Local migration-record validation

Migration-history validation delegates each loaded record to a directly following named helper. The
outer operation now reads as load history, validate records, and validate the supplied baseline. The
helper keeps row decoding, version lookup, and checksum comparison together with their distinct
error contracts. It caches the supported version locally instead of recomputing it for one error. No
migration execution or archive lifecycle policy changes.

All ten lifecycle/read-search integration scenarios, nightly formatting, rumdl, and changed-page
Markdown linting pass.

### Foreign-key probe and report ownership

The foreign-key health check now delegates connection acquisition and constraint probing to a
directly following named operation. The reporting function only converts the probe result into its
stable check. The probe contract explains connection locality, temporary state, and distinct failure
details. Error text and cleanup behavior remain unchanged.

All six archive-lifecycle scenarios, nightly formatting, rumdl, and changed-page Markdown linting
pass.

### Named FTS probe phases

The FTS5 health check delegates to explicit connection acquisition, initial cleanup, create/query,
and final cleanup operations. Result composition replaces three related booleans and preserves the
requirement that final cleanup runs after a create/query failure. Short contracts identify who owns
cleanup and why an initial cleanup failure stops the probe. Public reporting text is unchanged.

All six lifecycle integration scenarios, store all-target/all-feature Clippy, nightly formatting,
rumdl, and changed-page Markdown linting pass.

### Explicit foreign-key probe cleanup sequence

Foreign-key enforcement now reads as initial cleanup, named constraint exercise, final cleanup, and
result composition. The constraint operation documents the exact expected rejection and keeps SQL
execution together. Cleanup ownership is visible at the coordinating level, matching the FTS probe
without introducing a generic probe framework.

All six lifecycle scenarios, strict store documentation including private items, nightly formatting,
rumdl, and changed-page Markdown linting pass.

### Diagnostic count units and family ownership

Work diagnostics now explain that job, run, and failure counts measure different units and cannot be
summed. In-progress runs can be live or abandoned; family membership does not prove retryability.
Known-family counting has a named archive method with explicit ordering, unknown-label, and separate
read contracts. The outer work projection keeps its scalar counters and report assembly together.

All six lifecycle scenarios, store all-target/all-feature Clippy, nightly formatting, rumdl, and
changed-page Markdown linting pass.

### Shared store clock conversion

Archive creation, lease expiry checks, and diagnostics now import checked process-clock conversion
from a private store module. The owner documents truncation, range errors, wall-clock limitations,
and the distinction from observation ordering and fencing. Existing caller error mappings remain
unchanged; no public clock API or configuration dependency is added.

All 26 store unit tests, two lease scenarios, and six lifecycle scenarios pass. Nightly formatting,
rumdl, and changed-page Markdown linting pass.

### Store adapter navigation and clock gates

The architecture guide includes store clock ownership, health probe cleanup, and diagnostic count
units alongside the SQL adapters. Store all-target/all-feature Clippy and strict private-item
Rustdoc pass after the shared clock move (`/tmp/forgesync-store-clock-clippy.log` and
`/tmp/forgesync-store-clock-doc.log`). Rumdl and changed-page Markdown linting pass.

### Observation context accessor contracts

Core observation docs explain how missing versus invalid clocks affect downstream interpretation,
why empty complete and incomplete collections differ, and what each accessor does not establish.
Payload/family alignment remains a boundary responsibility; acquisition time is diagnostic and
sequence values are archive-local coordinates. These contracts put interpretation next to the values
callers inspect instead of requiring reconstruction of store policy.

The complete core suite passes: 28 unit tests, four catalog checks, and eight doctests. Nightly
formatting, rumdl, and changed-page Markdown linting pass.

### Named source-clock normalization scenarios

Core source-clock tests separate absent/empty/whitespace input, UTC offset normalization, and
malformed spelling retention. Named parameterized missing-input cases avoid scenario loops while
adding the previously implicit empty-string boundary. Observation tests import their types from the
owner and compare complete-empty state directly.

All seven focused observation scenarios, nightly formatting, rumdl, and changed-page Markdown
linting pass.

### Named document hash inputs

Document hash regression tests distinguish provider-timestamp changes from retrieval-input changes.
Recipe and rendered-text changes are separate named parameterized scenarios with one comparison
each. The construction fixture states that text is supplied directly rather than rendered from the
recipe, keeping the hash contract independent of engine materialization assumptions.

All three focused document-hash scenarios, nightly formatting, rumdl, and changed-page Markdown
linting pass.

### Embedding validation and encoding boundaries

Embedding contracts document validation precedence, unchanged magnitudes, and why matching
dimensions do not establish model compatibility. Numeric and encoding regressions use named linear
cases, including empty vectors, infinity, zero declared dimensions, and trailing bytes. Tests import
the value owner directly and retain exact typed error expectations.

All nine focused embedding scenarios, nightly formatting, rumdl, and changed-page Markdown linting
pass.

### Timestamp precision boundary scenarios

Timestamp archive round-trip, unsupported range, and discarded precision now have independent
scenarios. Named microsecond cases establish truncation toward the epoch on both sides, including a
negative sub-microsecond instant becoming zero. Signed extremes both reject unsupported instants.
These tests make the documented arithmetic boundary explicit without scenario loops.

All nine focused timestamp scenarios, nightly formatting, rumdl, and changed-page Markdown linting
pass.

### Core value review gates

All-target/all-feature core Clippy and strict Rustdoc including private items pass after
observation, document, embedding, and timestamp cleanup. Evidence is in
`/tmp/forgesync-core-review-clippy.log` and `/tmp/forgesync-core-review-doc.log`. The audit records
these concrete reviewed contracts while keeping the complete item-depth and suite-quality passes
open. Markdown gates pass.

### Failure classification and message producer contracts

Core failure docs distinguish typed categories from recovery policy and explicitly place message
safety with producers. Public fields and deserialization do not redact arbitrary text. Consumers use
workflow/ledger context rather than diagnostic prose to decide retry, and a failure does not prove
earlier writes were rolled back. Outcome tests import the defining module directly.

Strict core Rustdoc, all five outcome scenarios, nightly formatting, rumdl, and changed-page
Markdown linting pass.

### Reusable producer and accounting documentation rules

The linked documentation guide records two recurring review findings: producer obligations must not
be presented as runtime guarantees, and count contracts must identify their units/populations. It
uses message safety, completeness, compatibility, retry classification, active runs, and unresolved
ledger entries as concrete examples. These rules preserve future review context at the existing
AGENTS-linked entry point. Rumdl and changed-page Markdown linting pass.

### Nearby core value test files

Observation, timestamp, and document suites move from inline blocks to adjacent `tests.rs` files.
Each has a module introduction explaining its value boundary, scenario inputs, and what downstream
workflow tests establish separately. Production files now end at their contracts and algorithms;
focused tests stay one direct navigation step away with explicit owner imports.

The complete core suite passes: 46 unit tests, four catalog checks, and eight doctests. Nightly
formatting, rumdl, and changed-page Markdown linting pass.

### Local provider error-code classifier

The refreshed source inventory found a nested provider error mapping in engine output
classification. A named local classifier now handles every GitHub variant in one exhaustive match.
The outer engine match delegates to it and no longer carries unreachable cases for variants
classified earlier. Codes and policy are unchanged; classification is still distinct from retry
decisions.

The focused engine error-name selection passes all 22 matching scenarios, including error mapping
and error-preserving lease cleanup. Nightly formatting, rumdl, and changed-page Markdown linting
pass.

### Issue source-field retention ownership

REST issue normalization delegates raw extension/object retention to a method on the issue DTO. The
domain mapper keeps identity, display projections, timestamps, and discussion assembly together. The
directly following method documents which fields it takes and why labels/assignees must be projected
first. Existing retained keys, pull-request classification, and error mapping are preserved.

The complete GitHub suite passes: 24 unit tests and two doctests. Nightly formatting, rumdl, and
changed-page Markdown linting pass.

### Current review-head projection ownership

Coverage loading delegates current pull-request head lookup to a directly following query. The
helper owns membership payload decoding and documents that its read is separate from acquired head
context and completeness rows. Coverage assembly now follows the two source reads explicitly, while
preserving bound parameters, malformed-data errors, and freshness interpretation.

All 13 read/search and observation integration scenarios, nightly formatting, rumdl, and
changed-page Markdown linting pass.

### Scan traversal length disposition and failure contract

The refreshed syntax inventory leaves two production bodies above fifty lines: constant store error
classification and repository traversal. Both receive concrete retained dispositions in the audit.
Traversal docs explain individual parent commits, unadvanced cursor after page failure, replay
ordering, and failures that may return after durable progress. Persistence remains the mutation
owner; traversal retains the local page/cycle sequence.

### Shared argument diagnostic presentation

CLI parse dispatch points to a named argument-error presenter directly below the entry point. Usage
errors reuse the same stream/status handling. Its contract covers successful help/version outcomes,
printing failure, and the usage-code fallback; the dispatcher no longer hides that process policy in
a match arm.

All 14 CLI contract scenarios, nightly formatting, rumdl, and changed-page Markdown linting pass.

### Named tracing format installation

Logging format dispatch points to directly following text/JSON subscriber operations. Their
contracts explain stderr ownership, resolved verbosity, event flattening, and installation-failure
behavior. Tracing JSON is distinguished from command envelopes so callers do not confuse two
independent process-output policies. Subscriber settings and command status behavior are unchanged.

All-target CLI Clippy and all 14 CLI contract scenarios pass. Nightly formatting and changed-page
Markdown checks pass after prose normalization.

### CLI process fixture ownership and scope scenarios

Process-contract children import domain, store, and clock types from their defining modules. The
parent now supplies only executable/path/cleanup fixtures. Fixture contracts distinguish command
construction, filename allocation, and best-effort SQLite sidecar cleanup. Missing scope and
conflicting scope have independent named tests, so one failure no longer masks the other.

All 15 CLI process contract cases pass after the import and scenario changes.

### Sync scenario import ownership

All seven sync scenario children import external types, provider mocks, matchers, and workflow
operations from their defining modules. The shared fixture parent no longer supplies an external
prelude. Shared acquisition helpers still have positional family flags and remain explicit
candidates for the test-quality pass; this import cleanup does not close their behavior review.

Focused all-warning Clippy and all 15 sync workflow scenarios pass. Nightly formatting, rumdl, and
changed-page Markdown linting pass.

### Explicit acquisition in sync scenarios

The 24 calls previously routed through three acquisition helpers now construct `SyncRequest` beside
the real `sync_repositories` call. Family selection, scope, parent-run policy, cancellation, and
report failure expectations are visible in each scenario. The fixture parent only builds client
routing; it no longer executes acquisition through positional family flags. This resolves the
previous entry's acquisition-helper candidate and records the general rule in Rust conventions.

All 15 sync workflow scenarios and focused warning-denying Clippy pass. Nightly formatting, rumdl,
and linting of all three changed guidance/status pages pass.
