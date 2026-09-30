# Module map

Forgesync follows a local discussion from GitHub acquisition into an SQLite archive, then answers
offline reads and presents maintainer decisions. This map names the owner of each transition.

| Crate              | Owns                                                             | Main path                                   |
| ------------------ | ---------------------------------------------------------------- | ------------------------------------------- |
| `forgesync-core`   | Identities, normalized content, observations, coverage, outcomes | Provider values become domain values        |
| `forgesync-github` | Typed responses, transport, pagination, normalization            | GitHub response becomes a typed observation |
| `forgesync-store`  | Archive lifecycle, SQL, ordering, recovery, local decisions      | Observation becomes durable state           |
| `forgesync-engine` | Sync, refresh, search, clustering policy                         | Request becomes a workflow report           |
| `forgesync-cli`    | Arguments, configuration, process output and exit codes          | User command becomes an engine request      |
| `forgesync-tui`    | Local navigation, actions, rendering                             | Archive state becomes an interactive view   |

For a sync change, start at the CLI command, follow the engine sync operation through the GitHub
resource method and store application method, then inspect the matching fixture and regression test.
CLI parsing and execution share `crates/forgesync-cli/src/command/`; the parsed command type owns
its process `run` method. Shared configuration and result rendering remain separate modules. For an
offline read, start at the engine request, inspect the store query, then the CLI or TUI
presentation. The engine accepts an opened archive; the store alone decides transaction and
observation ordering. GitHub code does not open the archive.

Within engine sync, `review_collection` owns the durable lifecycle shared by reviews and review
threads. `ReviewSync` selects the family and prepares against a metadata result; preparation either
finishes immediately or yields a reserved, head-aware `ReviewCollection`. The collection owns
staging counts and consuming completion/failure operations. `reviews` follows REST page links, while
`review_threads` owns GraphQL cursor traversal and cycle detection. Provider traversal and archive
finalization can therefore be read independently without repeating their shared policy.

The sync root defines the public request, progress, and report vocabulary. `sync/coordinator` owns
request preparation, durable scope serialization, job execution, and terminal run projection.
`sync/scope` defines shared run capabilities, independent enumeration units, and thread-family
attribution/results. Collectors import those definitions directly; failure recording belongs to the
thread scope and progress publication to the run context. `sync/lease` owns writer-fence
acquisition, renewal, cancellation draining, and release. Run-wide sync counters and outcome policy
live in `sync/accounting`, beside direct complete, partial, deferred, failed, and interrupted
scenarios. Repository sync uses `jobs` for lookup and scope traversal, `thread_job` for durable
parent scans, and `repository_work` for immutable services and selected scope. `comment_job` owns
repository-wide comment accounting; `comments` owns a reserved per-discussion collection.
`pull_requests` owns selected metadata/review jobs, and `family_job` holds their IDs, accumulated
results, and terminal ledger writes. `metadata` reserves and applies the head observation before
review acquisition.

Within the store, `observations/apply` selects a canonical parent and applies its independently
ordered evidence; `observations/thread_rows` owns the payload binding map and its private read/write
representations. `StoredThreadObservation` keeps canonical content and complete-evidence positions
independent. `ThreadPayloadUpdate` makes an optional evidence advance explicit.
`families/application` checks reserved generations and applies complete membership or incomplete
coverage inside the transaction opened by `families/finish`. These owners borrow the transaction and
never commit it.

Refresh `coordinator` binds services and validated repository scope to `RefreshExecution`; its stage
methods preserve independent reports. TUI `query/operations` owns task and message lifetimes, while
`query/action` owns the selected action request and terminal status.

Clustering `candidates` validates stable input, `evidence` selects sparse eligible edges,
`references` interprets title/body mentions, and `components` applies bounded grouping and
representative policy. Evidence selection shares one score predicate across both selection phases.

Store thread reads let `ThreadQuery` build bound filters and sorting before page coverage assembly.
Embedding `read` keeps raw candidate order, hydrates current evidence, and accepts only complete
valid chunk groups. Raw candidates determine pagination even when every vector is rejected.

GitHub transport `request` owns budgeted attempts and trusted redirect traversal; `client` owns
construction and endpoint entry points. CLI `command/embedding_service` prepares configured clients
without making provider requests, preserving configuration versus initialization failures.
`command/interruption` scopes the Ctrl-C listener and lends its token to command workflows; engine
operations retain ownership of interrupted reports and durable cleanup. CLI `reports/detail` and TUI
`view/detail` build named presentation sections from loaded projections.

CLI `command/progress` owns the bounded advisory channel and stderr task shared by sync and retry.
It closes local delivery before draining and aborts the task when its command owner is dropped.
`command/retry` owns durable failure selection and closes its archive before rendering any outcome.
Engine `runs/planning` interprets recorded scope, resolves failure repositories, and merges and
orders minimal retry requests without provider I/O; `runs` executes those requests through sync.

The crates expose named concept modules rather than blanket root exports. The main sync, search,
cluster, storage, GitHub transport, CLI command, and TUI rendering paths are grouped by behavior.
The [maintainability plan](maintainability-plan.md) records the ownership migration, and the
[source shape audit](source-shape-audit.md) records fixed and retained review dispositions. Use the
module path as the first navigation clue, then read the adjacent tests.

## Local query boundaries

CLI `command/thread_filters` owns the parsed repository, kind, state, ordering, and page choices
shared by thread listing and search. `ThreadFilterArgs` converts that cohesive input into engine
filters; query text and retrieval policy remain with the search command. Argument parsing supplies
basic ranges, while the engine validates requests independently of Clap.

Engine `inspect` owns public inspection vocabulary and local read operations. Its private `query`
module adapts that vocabulary for inspection, search, and cluster browsing: registered-name
resolution, bounded pagination, and store enum conversion. The module boundary hides these helpers
without adding restricted visibility to each function. Store `reads` owns SQL and projection
assembly. Repository resolution and projection reads are separate operations, not a frozen snapshot.

Start at the command when changing flags, at the engine request when changing workflow policy, and
at the store projection when changing selection or decoding. Nearby filter tests establish parsing
and conversion; engine and store scenarios establish archive behavior.

## Shared engine policy owners

Private `clock` acquires wall time for source acquisition, derived writes, and lease coordination.
It does not define durable ordering: observation sequences and store rules do that. Private
`provider_failure` maps typed GitHub failures to recorded domain categories; workflow owners decide
retry and cancellation handling before choosing whether to record those failures.

Public `exact_search` exposes cosine arithmetic. Private `scoring` owns ranked discussion
candidates, best-chunk filtering, bounded merging, and stable identity ordering shared by search and
clustering. Vector eligibility remains with the workflow/store selection boundary. Arithmetic and
scoring tests live beside their respective owners so fixtures and policy assertions do not mix.

## Private store adapters

`observation_sql` owns clock-column conversion, checked SQLite integers, canonical row lookup, and
coverage persistence shared by parent observations, child families, and run records. Public domain
inputs/results remain in `observations`. These helpers borrow the caller's connection; ordering,
lease checks, and transaction commit remain with the archive operation.

`coverage_projection` loads recorded completeness and current head context, then derives visible
staleness without changing durable coverage. Thread reads, embedding eligibility, and cluster
members import it directly. `query_sql` supplies bound repository/kind/state predicates shared by
those reads; each query still owns aliases, joins, ordering, page windows, and decoding.

Store `clock` owns checked process wall-clock conversion shared by archive creation, diagnostics,
and lease expiry checks. It truncates to archive microseconds and rejects pre-epoch or overflowing
values. It supplies observations only: transaction fences and observation sequences still establish
writer validity and acquisition ordering.

`health` keeps operator-facing check construction above named connection-local probes. Constraint
and FTS execution borrow the acquired connection, while their coordinators attempt final cleanup
before combining results. `diagnostics` keeps job/run/failure units distinct and reads known-family
counts through its named projection; these separate reads do not establish a frozen snapshot.

These private modules make implementation dependencies visible without adding SQL resources to
public library APIs. Start in the workflow or public archive method, then follow its named adapter
when changing column conversion or a genuinely shared selection rule.

## Child-family transaction phases

Reservation, staging, and finalization share one ordering contract but own different effects.
`ChildFamilyRequest` names the parent, evidence family, source clock, local start, and provider
scope at the acquisition boundary. Archive reservation accepts that declaration plus a separate
writer token when fencing is required; construction alone performs no validation or write.
`ChildFamilyPage` names one provisional member slice and page index within the accepted reservation.
`ReservedGeneration` compares a proposed source clock and sequence, then writes a reservation and
recoverable generation. `PageWrite` validates that generation, recognizes identical replay, and
stores provisional pages with received counts. `FamilyApplication` promotes complete membership or
records partial coverage. Each archive operation opens and commits its own transaction; these owners
only borrow its connection.

Reuse is a separate read. `MembershipExpectation` names the independent evidence available from the
parent, and `FamilyFreshness` verifies source clock, review head when required, complete coverage,
and canonical membership count. None of these reads promote staged pages or change coverage.

## Terminal picker and writer display state

`app/repositories` owns repository rows, highlight, applied scope, and the pending read generation.
The synthetic all-repositories row is a cursor position; the applied filter retains a repository
value independently of refreshed row order. A matching provider identity refreshes its metadata,
including renamed repository URLs; an absent row does not broaden the applied scope. The owner
rejects stale replies, clamps cursor bounds, and retains loaded rows when a refresh fails. App
coordination resets thread and detail state when the user applies a new scope.

`app/operation` owns the displayed writer generation and an idle/running enum. A running state
contains its required label and optional progress snapshot. Input reads whether cancellation is
needed before quitting; the view reads presentation facts. Query tasks still own execution and
cancellation. The display accepts current-generation progress only while running, and completion
clears transient state before returning the status to the app.

## Terminal read panel ownership

| Module         | Owns                                           | Cache and selection rule                                         |
| -------------- | ---------------------------------------------- | ---------------------------------------------------------------- |
| `app/threads`  | Discussion page, offset, continuation, cursor  | New read removes old rows; current success selects the first row |
| `app/detail`   | Selected discussion, generation, scroll, state | Invalidation rejects old replies and removes old content         |
| `app/coverage` | Archive-wide projection and refresh state      | Pending or failed refresh retains the last successful projection |
| `app/failures` | Retry choices, read state, highlighted run     | Refresh retains rows; replacement clamps cursor bounds           |
| `app/clusters` | Cluster choices and selected member projection | Another cluster clears old members; same-cluster refresh retains |
| `app/messages` | Typed query results and writer updates         | Generations remain attached to the owning result                 |

`ThreadReply` carries offset and generation with its page rather than making the app reconstruct
request coordinates. Detail has mutually exclusive empty/loading/ready/failed states. App
coordination starts a list read and invalidates detail together; standalone reads begin on their
panel owners. Query tasks own archive access, and views read projection and loading/error facts.

Cluster detail keeps members only while refreshing the same cluster. Opening another cluster clears
old data immediately, so local canonical/exclusion keys cannot target its previous members while the
new selection is loading. Decisions still come from the archive after a writer ends. Fixed
repository/cluster test data is shared only where those transition scenarios use the same values;
rendering-specific data remains with the renderer tests.

`QueryDispatch` borrows one session's archive, clients, runtime, reply channel, and task registry
for an event-loop dispatch batch. Its methods receive action and app state, replacing positional
service lists without taking over resource lifecycle. Within dispatch, `requests` carries intent and
`tasks` owns background lifetime. `reads` binds shared local scheduling services; `thread_page` owns
browse/keyword filter preparation and `failures` bounds recent ledger selection. Failure projection
belongs to `RunFailureSummary`, not to the dispatcher. Writer `progress` keeps producer and
forwarding task together, drains before completion delivery, and aborts on unexpected drop. These
owners keep runtime lifetime, read policy, and navigation state distinct without repeating shared
service parameters in every read starter.

The private `event_loop` module binds the live app, completion channel, runtime, task owner,
archive, and clients for one terminal session. Its coordinator drains messages, draws, and polls
input; operation completion dispatches panel refreshes before the next frame. The crate launcher
retains terminal setup/restoration, awaited task shutdown, and archive closure.

Presentation projections borrow loaded data. `view/coverage` assembles identity, evidence, health,
and lease sections; `view/clusters` keeps detail selection with member-line formatting. Both CLI
`reports/timeline` and TUI `view/timeline` name event wording and missing-data fallback separately
from detail section assembly. They preserve store event order and start no acquisition or query.

CLI command and report roots import only what they use; child code names its owning module rather
than a parent alias. Workflow-specific output DTOs live with their report formatters. Refresh
presentation selects stage formatters in request order, then renders each existing `RefreshStage`
through the common status/detail/failure pattern. Report preparation stays pure and separate from
archive cleanup and output stream policy.

Clustering keeps graph evidence and bounded proposals with their algorithms: `clustering/evidence`
owns the neighbor heap and selected edges, `components` owns bounded union-find groups, `proposals`
owns representative selection and proposed members, and `references` owns mention parsing and
title-token policy. These representations use indexes into one stable document snapshot; they are
not durable archive identities. The clustering root presents requests, reports, and shared options,
while child modules import store and runtime types directly.

CLI refresh keeps parsed choices with request preparation. Its `PreparedRefresh` pairs ordered
engine stages with the optional embedding capability selected for those stages. Credential setup is
skipped for local-only refresh, and the outer command closes the archive before rendering typed
setup or engine failures. An unusable optional embedding service remains a structured selected-stage
failure, so earlier sync or analysis evidence can still be reported.

Embedding process presentation separates an absent engine stage report from a present report with
partial, deferred, or interrupted work. The command owns missing-report diagnostics;
`reports/embedding::EmbeddingOutput` owns exit selection from the same state exposed in JSON. Both
paths present results after the writable archive is closed.

CLI embedding preparation retains repository scope, client identity, recipe, replacement policy, and
dimensions in `PreparedEmbedding`. It executes against an opened archive and projects the same
identity into its result; the outer command owns archive closure and presentation. Configuration
setup happens before opening, while actual model requests remain in the engine stage.

Engine `embeddings/chunks` owns deterministic text splitting and compatibility of persisted chunks
with current document input. Its value identity includes position, total count, hash, and text. The
embedding workflow uses those values to schedule requests and persist vectors under a writer fence;
chunk construction itself performs no provider or archive I/O. Nearby chunk and request-batch tests
cover their separate contracts.

Engine `embeddings/selection::EmbeddingSelection` owns source-version deduplication, service-scoped
archive reads, and selected/reusable chunk accounting before writer lease acquisition. Pending
`EmbeddingTask` values retain a shared full document with each chunk for later fenced persistence.
The public coordinator consumes the selected report and tasks, then owns lease execution and
release.

Embedding execution now has three private owners beside selection and chunk identity. `batches`
groups requests and retains input order through provider responses. `scheduling::BatchScheduler`
owns pending work, concurrency, outcome dispatch, and worker draining. `execution::EmbeddingWriter`
keeps the archive fence with its service identity, renews before each chunk write, and releases only
after scheduling finishes cleanup. Provider failures remain report entries; worker/persistence
errors abort and drain outstanding requests before returning the original error.

Cluster generation holds `clustering/lease::ClusterBuildLease` across vector loading, blocking
analysis, and generation persistence. The owner renews the fence and keeps a child cancellation
scope. Caller interruption or renewal failure cancels that child and awaits analysis cleanup before
release; the original triggering failure is retained. Short local decision writes continue to use
the release helpers without taking on the long-build renewal lifecycle.

CLI `command/cluster/build` implements preparation and execution on `ClusterBuildArgs`. Preparation
combines parsed graph policy with canonical configured endpoint/model identity and recipe, without
resolving a secret or creating a model client. The command opens the archive only after
configuration validation, closes it after the engine result, and delegates report or failure
presentation.

Refresh `clusters::ClusterStage` retains one service identity, recipe, and graph policy across
repository traversal. It owns attempted outcomes, first-diagnostic retention, and partial-coverage
accounting. Repository failures remain isolated; cancellation stops further attempts without
removing earlier generations. Aggregate status follows the retained first failure and successful
coverage evidence, with nearby tests documenting that policy independently of archive setup.

Cluster `snapshot::ClusterSnapshot` owns resolved repository identity, eligible open-thread count,
compatible-vector count, and ordered document/vector evidence. It rejects unavailable or
inconsistent evidence before graph analysis. Thread counts use offset pages; vector reads retain the
store's raw cursor and one fixed build request identity. The generation coordinator consumes that
snapshot under its existing lease and projects candidate membership through a named store-input
conversion.

Store `clusters/generation_input` validates proposed membership before transaction creation, then
resolves source identities inside the active transaction into sorted `PreparedCluster` rows. Local
decisions share its discussion-row resolver. `generation_matching` loads existing active/excluded
membership and assigns durable IDs by ordered overlap. Neither module commits or performs generation
writes; `generation` retains the fenced transaction and its commit boundary.

Durable matching uses `generation_matching::MembershipOverlap` to name shared count, union count,
generated position, and existing row ID. Candidate enumeration, priority comparison, and greedy
one-to-one assignment are separate local operations. Absolute overlap takes precedence over
proportional overlap; exact fraction comparison and stable row/index tie breaks preserve repeatable
identity reuse. Nearby static-membership cases cover assignment without SQL setup.

Store `generation_apply::GenerationApplication` owns one run's repository identity, timestamp,
coverage policy, seen cluster rows, and membership accounting. It orders cluster/member application,
complete-scope retirement, and run finalization. `generation_rows` keeps the underlying SQL bind
maps linear. `Archive::save_clusters_fenced` retains transaction creation, fencing, and the single
commit; helpers never commit. Result conversion follows commit under the existing outcome contract.

Search `fusion::HybridRanking` merges candidates by durable discussion identity, retaining the first
summary and each source's rank evidence. `SemanticEvidence` binds semantic rank to its cosine
explanation. `FusionEntry` projects reciprocal-rank scores and keyword-then-semantic provenance;
explicit ordering adds the requested sort and stable identity tie break before truncation.
Pagination and fallback classification remain in `ranking`, separate from source fusion. Its
`ResultPageRequest` describes an already ordered prefix and projection metadata rather than the
original user request. `keyword::KeywordCandidates` owns prefix accumulation and coverage; keyword
page projections interpret the existing `SearchRequest` directly.

Ranked search coordination lives in `search::ranked`. `RankedSearch` keeps request interpretation
and the validated `SearchWindow` together during vector acquisition and page construction. The
window bounds the offset-plus-page prefix and adds a continuation probe. Named semantic and hybrid
projections apply cosine evidence or fusion before pagination; the public search module selects the
workflow and retains its public request/result types.

Repository enumeration keeps provider traversal in `enumeration::scan`, reserved archive mutations
in `scan_persistence`, and terminal coverage/diagnostic meaning in `scan_outcome`. Earlier parent
observations remain usable after failed or cancelled traversal; only a durably recorded terminal
page permits complete scan coverage. The report is read after the terminal write. Store
`enumeration::completion` validates terminal status/failure combinations and the active-generation
cursor before writing terminal evidence within the archive-owned transaction.

Store read projections separate individual discussion evidence (`coverage_projection`) from
archive-wide counts and status (`reads::summary`). Summary accumulation validates grouped SQL
labels/counts while preserving explicit missing/incomplete/complete buckets. Aggregate status uses
separate read queries; it provides diagnostics rather than a transactionally frozen snapshot of
concurrent writers.

## Engine workflow scenario fixtures

The sync integration suite keeps real acquisition requests and operations in each scenario. Shared
setup under `tests/sync_scenarios` has three sibling owners: `fixture_issues` for REST discussion
responses and local clients, `fixture_reviews` for pull-request head/review responses, and
`fixture_archive` for checked references, already-read coverage selection, and archive lifetime.
Scenarios read current detail and archive status directly; fixtures perform no archive reads or
writes and do not run engine workflows. Document revision setup lives in
`fixture_documents::DocumentSource`, whose named clocks and body configure provider responses
without acquisition. The document scenario keeps actual sync and materialization visible. Review
regressions have sibling owners for failure isolation, head freshness, membership replacement, and
partial-collection isolation, keeping each complete scenario local without deeper module nesting.

Comment sync regressions have sibling owners for stale retry, empty-membership isolation, and
failure-ledger abort. The retry case retains its dependent acquisition phases together. Empty
membership compares two issues in one run to show sibling failure isolation; the ledger case targets
preservation of both provider and database failure causes.

Sync enumeration regressions have sibling owners for cancellation/replay, closed-sweep watermark
recovery, and failed-run writer-fence cleanup. Replay's responder notification identifies the second
page boundary directly; closed sweep owns overlap and checkpoint fault setup; run creation needs no
provider fixture because its foreign-key rejection precedes acquisition.

Embedding integration has sibling retry, hybrid-search, and keyword-fallback owners. Each constructs
its own explicit archive/vector precondition instead of reaching search through an unrelated failed
batch. `fixture_embeddings` owns only deterministic service response policy and client construction;
its counters expose HTTP attempts while archive operations remain in the scenarios.

Cluster integration under `tests/clustering_workflow` has shallow complete, partial, and namespace
scenario owners. Its fixture module constructs identities and source/document values only, plus path
allocation and cleanup. Scenarios explicitly reserve observations, apply source evidence, persist
fenced document/vector chunks, release the preparation lease, and invoke engine operations. The
partial case retains its complete baseline because subsequent preservation depends on it.

Enumeration integration under `tests/thread_enumeration` separates page-failure and replay contracts
from client/payload setup. Scenarios invoke enumeration and archive reads directly, comparing
durable scan records with reported state. Replay keeps its initial read as the explicit content
baseline; page failure distinguishes completed parent observations from missing child coverage.

### Closed-sweep publication

The store's public `checkpoints` module owns `ClosedSweepCheckpoint`, the typed input for publishing
a completed scan's source boundary. The engine parent-thread job constructs repository, scan
sequence, source boundary, and publication time together. `Archive` owns validation and commit; the
lease token remains separate from checkpoint data. Checkpoint reads still return the retained
source-time boundary without acquiring provider data.

### Offline CLI query scenarios

The `offline_queries` process suite has shallow sibling modules for keyword search, advanced FTS,
thread selection, argument validation, and human presentation. Each query scenario constructs its
own archive and shows its command and assertions directly, including reported-state comparisons
before and after execution. `fixture` owns the fixed repository/observation construction, unique
paths, and closed-handle cleanup; it does not execute queries. Store and engine suites establish
query mechanics, while these cases establish process output and validation boundaries.
