# Implementation status

## Current position

- P6.1 implementation is in place; hosted Windows, Linux, and Intel macOS platform results remain
  to be collected by CI.
- Next action: **Run the hosted platform matrix before preparing a release**.
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

## P2.1 evidence

Added the `forgesync-github` crate with a shared HTTPS-first Reqwest client, a four-request
concurrency limit, 30-second request timeout, five-attempt limit, and bounded total retry budget.
Requests use a redacted token type, bound JSON response bodies, and emit tracing spans with only
the configured origin, method, attempt, and status. HTTP errors are typed without retaining raw
provider payloads.

The client attaches bearer credentials only to its configured origin. It validates absolute and
relative pagination URLs, disables automatic redirects, and explicitly follows same-origin redirects
after validating each destination. Cross-origin redirects are rejected before contact. It retries
network failures, 429, selected 5xx responses, and identified 403 rate limits; generic
authentication and permission failures are not retried. Retry and reset hints are honored when they
fit the retry budget; otherwise the operation is deferred.

Added CLI-edge credential discovery in configured environment variable, `GITHUB_TOKEN`, then
host-aware `gh auth token --hostname HOST` order. The `gh` subprocess uses argument arrays, suppresses
stderr, and has timeout and cancellation handling. Local archive commands do not invoke credential
discovery.

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
each thread row independently, and advances the cursor only after every row in that page commits.
A later page failure or cancellation leaves prior rows available and the scan explicitly incomplete.

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

Added paginated REST issue-comment acquisition and normalization, preserving unknown provider fields.
`sync --with comments` selects comment acquisition; default sync makes no comment requests. Each
repository/state scope receives a separate comments job. Complete observations atomically replace
membership, including with an empty set. Incomplete observations preserve previous membership and
expose incomplete coverage. Complete snapshots are reused only while the parent timestamp, expected
comment count, and stored membership count all match.

Thread-specific failures are persisted independently so one failed thread does not roll back
successful siblings. Retries skip fresh siblings, update prior retry records, and resolve failures
only after matching complete coverage exists. A failure-ledger write error reports both the
original provider failure and the local write error.

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
review-thread connection, and fully acquire each nested comment connection before returning a
page. A page with a nested pagination gap is not representable as a complete page. GraphQL `errors`
are checked even when HTTP succeeds and partial `data` is present; missing or repeated cursors fail
the selected family observation.

Added `sync --with review-threads` as an independently reported pull-request evidence family.
Complete snapshots bind current resolution, outdated state, and nested comments to the currently
observed PR head. Incomplete results retain last complete membership and head provenance. Complete
empty results remove current membership, restoration re-adds it, and a changed head makes prior
review-thread coverage stale. GraphQL family failures do not change comment or review membership.

Local HTTP, sync, and archive tests cover enterprise GraphQL paths, typed outer pagination, nested
comment pagination, partial errors on both connection levels, failed refresh preservation,
complete removal and restoration, head staleness, and transaction rollback of membership, coverage,
and head context. CLI parsing covers `--with review-threads`.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`:
  passed.
- `cargo test --workspace --all-features --locked --offline`: passed, 90 tests.
- `cargo build -p forgesync-cli --no-default-features --locked --offline`: passed.
- `cargo doc --workspace --no-deps --all-features --locked --offline`: passed.
- `markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml docs/compatibility.md
  docs/implementation-status.md`: passed, 0 issues.

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
- `markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml docs/compatibility.md
  docs/implementation-status.md`: passed, 0 issues.

## P4.1 evidence

Added two deterministic document recipes: `original_body` and `discussion_enriched`. The enriched
recipe adds current non-bot comments, submitted reviews, and review-thread state and comments.
Documents carry a recipe version, host-qualified source identity, SHA-256 content hash, normalized
deduplication text, and source update timestamp. Retrieval timestamps are stored separately from
the content hash. The store validates the recipe version and recomputes the hash before persistence;
document writes run under the archive writer fence.

The CLI loads optional TOML configuration from `--config PATH` or `FORGESYNC_CONFIG`. The
`[documents].recipe` setting defaults to `discussion_enriched`; unknown fields and recipe names
are rejected. See [configuration.md](configuration.md).

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
chunk index, and chunk hash. A batch commits independently; retries select only missing chunks
whose document and service identity remain current. Enriched documents exclude stale comments,
reviews, and review-thread evidence.

Local fixtures cover out-of-order and malformed responses, unsafe redirects, byte-bounded chunking
and batches, and a later batch failure followed by a retry that requests only the missing chunk.
The CLI also confirms that an empty repository selection needs no provider request.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 115 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.
- `markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml docs/configuration.md
  docs/compatibility.md docs/implementation-status.md`: passed, 0 issues.

## P4.3 evidence

Added paged exact semantic retrieval over current vectors matching the configured endpoint, model,
recipe, document hash, source update, and enriched-evidence freshness. Each bounded page is scored in
a blocking worker under a process-wide concurrency limit; cancellation is checked while ranking and
between pages. A discussion with multiple document chunks receives its maximum chunk cosine score.
Hybrid retrieval fuses keyword and semantic ranks with reciprocal rank fusion (constant 60), and
results report the requested/effective mode, ranking, scores, and contributing source ranks. Missing
vectors or provider failures remain explicit unless the caller selects keyword fallback.

The CLI exposes semantic and hybrid modes plus `--keyword-fallback`. Keyword search remains local
and independent of embedding configuration. Local fixtures cover known cosine directions, chunk
maxima, dimension rejection, stable ties, cancellation, RRF scores and provenance, missing model
configuration, and fallback without an extra provider request.

The reproducible exact-cosine benchmark uses the production cosine and ranking implementation with
deterministic 1536-dimensional f32 vectors. Run `cargo build --release -p forgesync-engine
--example exact-cosine-benchmark --locked`, then `/usr/bin/time -l
target/release/examples/exact-cosine-benchmark 10000 1536` and repeat with `100000`. On an Apple M2
Max with 64 GiB RAM, macOS 26.6.2, and rustc 1.98.1, ranking plus top-20 sorting took 20.192 ms
and 204.010 ms, respectively. Peak process RSS was 69,173,248 bytes at 10k and 636,764,160 bytes
at 100k; each vector set contains 61,440,000 and 614,400,000 raw input bytes. Generation and
startup are excluded from the reported ranking time but included in peak RSS. This measures in-memory
cosine ranking and sorting, not SQLite reads or embedding-provider latency.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 120 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.
- `markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml docs/configuration.md
  docs/compatibility.md docs/implementation-status.md`: passed, 0 issues.

## P4.4 evidence

Added deterministic candidate graph construction from current stored vectors and explicit issue
references. The graph retains Gitcrawl's selected scoring gates: same-kind cosine threshold `0.80`,
cross-kind threshold `0.93`, high-confidence threshold `0.90`, weak title overlap `0.18`, direct
reference score `0.94`, and early body reference evidence within 240 bytes. Per-thread fanout defaults
to 16, connected components are capped at 40 members, and representative selection breaks degree
ties by issue number and stable identity. Graph construction is separate from SQLite persistence and
CLI rendering, runs in a bounded blocking worker, and checks cancellation.

Migration 10 adds cluster runs, stable public cluster IDs, generated memberships, local member
decisions, and decision events. Regeneration matches old and new groups by deterministic member
overlap, keeps the same public ID where groups correspond, and carries canonical, exclusion, and
dismissal decisions forward. Partial vector coverage updates only observed groups and never retires
unseen groups or removes unseen memberships; complete current coverage may retire them.

The engine clusters current open discussions using only complete vectors for the selected endpoint,
model, and document recipe. It holds and heartbeats the archive writer lease across its local vector
snapshot, graph build, and generation write. It does not contact the embedding provider or read an API
key. Reports show eligible and vector counts; zero vectors with eligible discussions return an
actionable unavailable-vector error without writing a run. The CLI implements `cluster build`,
`cluster list`, `cluster show`, `cluster dismiss/restore`, `cluster exclude/include`, and
`cluster canonical` with versioned JSON and local-only decisions.

Reference-scoring unit tests cover similarity safeguards, repository-scoped references, early body
references, bounded fanout, max size, determinism, and cancellation. On-disk store tests cover stable
IDs, partial retention, complete retirement, canonical/exclusion/dismissal persistence, and input
validation. Engine and CLI integration tests cover fresh-vector selection, stale-vector partial
coverage, no-vector rejection, offline build/list, and operation without an embedding API key.

Validation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --workspace --all-features --locked`: passed, 128 tests.
- `cargo build -p forgesync-cli --no-default-features --locked`: passed.
- `cargo doc --workspace --no-deps --all-features --locked`: passed.
- `markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml docs/configuration.md
  docs/compatibility.md docs/implementation-status.md`: passed, 0 issues.

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
- `markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml docs/configuration.md
  docs/compatibility.md docs/implementation-status.md`: passed, 0 issues.

## P5.1 evidence

Added the `forgesync-tui` crate and enabled its optional CLI feature in normal builds. The terminal
browser lists registered repositories and discussions, scopes the discussion list to a selected
repository, opens current discussion detail, searches locally by keyword, and shows archive coverage
and recent failed or unfinished runs. Discussion pages contain 100 rows and can be traversed with
`n` and `p`. Narrow terminals stack the three panes; wider terminals place them side by side.

The TUI uses engine read APIs only. Repository, thread, detail, coverage, and run queries execute
on Tokio tasks while the render loop polls input and draws independently. Results use a bounded
message channel, carry a generation number, and stale results are discarded. Exit aborts and joins
outstanding queries; Ratatui terminal handling restores the alternate screen and cursor on normal
return and installs panic restoration.
The CLI refuses non-interactive use and `--json` for the TUI. It does not load app config or
embedding credentials for this local-only view; `--no-default-features` omits the command. See
[tui.md](tui.md).

Behavior was adapted from selected Gitcrawl TUI references, including
`TestTUIUpdateCoversKeyboardStateMachine`, `TestTUIRepositoryPickerSwitchesRepository`, and
`TestTUISyncComponentsClampsDetailViewportAfterResize` in
`internal/cli/tui_test.go`. Ratatui `TestBackend` tests render fixture data at 40×10 and 140×40 and
verify detail scrolling clamps after a resize. Reducer tests cover repository selection, search,
responsive keys during pending queries, and stale-result rejection.

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

Added generated-cluster and member browsing to the TUI, including lifecycle, local dismissal,
member inclusion state, canonical role, and generated neighbor scores. Cluster dismissal/restore,
member exclude/include, and canonical selection call the same engine methods used by CLI commands.
The failures view now selects a durable run for explicit retry through `plan_run_retry` and
`run_retry`.

The CLI opens the existing archive read-write and resolves GitHub clients at the CLI boundary.
`s` calls `sync_repositories`; `R` calls the shared `refresh` API for full GitHub evidence; `t`
retries the selected run. These actions run in background Tokio tasks. Engine progress remains
non-blocking under a slow terminal, and the TUI reports actual complete, partial, failed, deferred,
and interrupted results. Pressing `q` or `Ctrl+C` during work cancels through a token and waits for
the engine's terminal report, leaving the result visible before exit.

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
- `markdownlint-cli2 --config /Users/joshka/.markdownlint-cli2.yaml docs/tui.md
  docs/compatibility.md docs/implementation-status.md`: passed, 0 issues.

## P6.1 evidence

Reconciled the release description against the selected feature ledger. Native packages contain only
the Forgesync executable; cloud and portable distribution, code indexing, summaries, metrics,
analytics, legacy import, full revision history, deep PR details, and GitHub write-back remain
deferred. Forgesync keeps a separate archive format, and no Gitcrawl database or installation was
opened or changed.

Added installation and operations guidance for fresh archive setup, credential-free offline
queries, evidence coverage, interrupted-run recovery, diagnostic tracing, optional model settings,
and the TUI. Added Rust examples that call the engine's `search_threads` and `sync_repositories`
APIs directly. The CLI now installs its tracing subscriber at process startup, routes text or JSON
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
- Local Apple Silicon release build and `scripts/smoke_binary.py`: passed; SQLite integrity,
  foreign keys, FTS5, and offline keyword search all passed without provider credentials.
- Local Apple Silicon package creation and archive member verification: passed.
- `offline_search` example run against a fresh temporary archive: passed without credentials.
- `actionlint .github/workflows/ci.yml .github/workflows/release.yml`: passed.
- `rumdl fmt .`, `rumdl fmt --check .`, and `rumdl check .`: passed, 8 Markdown files.
- Markdownlint for `README.md`, `docs/installation.md`, `docs/releasing.md`,
  `docs/compatibility.md`, and `docs/implementation-status.md`: passed, 0 issues.
- Hosted Windows, Linux, and Intel macOS jobs are configured but have not run from this checkout.
  No GitHub release has been published.

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
| P2.1 — HTTP transport and credentials | Complete | Retry, origin-safe auth, cancellation, credential discovery |
| P2.2 — Thread enumeration | Complete | Stable identities, durable page cursors, rename handling, replay and partial-failure checks |
| P2.3 — Runs, leases, and basic sync | Complete | Fenced writes, run reports, cancellation/replay, closed-sweep overlap |
| P2.4 — Comments and independent failures | Complete | Paginated comments, stale coverage, isolated failures, selective retry |
| P3.1 — PR metadata and reviews | Complete | Head-bound review coverage, normalized reviewer identity, independent failure/retry |
| P3.2 — Review threads | Complete | Typed GraphQL pagination, nested completeness, head-bound state and rollback |
| P3.3 — Legacy import | Deferred | Reserved task; keep the Go archive intact |
| P3.4 — Health and explicit retry | Complete | Read-only diagnostics and selected failed-family retry |
| P4.1 — Versioned documents | Complete | Two recipes, timestamp-independent hashes, fenced document persistence |
| P4.2 — Embeddings | Complete | Bounded client, deterministic chunk batches, validated vectors, retry reuses successful batches |
| P4.3 — Semantic and hybrid search | Complete | Paged exact cosine, RRF provenance, explicit fallback, 10k/100k latency and peak-RSS measurements |
| P4.4 — Clustering and maintainer decisions | Complete | Bounded deterministic graph, durable stable IDs, local decisions, partial-coverage safety |
| P4.5 — Refresh composition | Complete | Optional shared-engine stages, durable partial reports, explicit model selection |
| P5.1 — Read-only browser | Complete | Responsive background queries, repository/thread detail, local search, coverage and failure views |
| P5.2 — Maintainer actions/live progress | Complete | Engine-backed cluster decisions, sync/retry/refresh, non-blocking progress, and writer ownership |
| P6.1 — V2 scope and packaging | Implemented; hosted matrix pending | Local Apple Silicon smoke/package pass; collect native Linux, Intel macOS, and Windows CI results before release |

A task is complete only when its acceptance checks pass. Keep deferred capabilities absent from code,
workspace members, runtime dependencies, command help, and schema.
