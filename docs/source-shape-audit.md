# Source shape audit

This inventory records the command ownership, function documentation, and dispatch review across all
six crates. It distinguishes a large match that hides behavior from a compact match that states
policy. Keep this as maintenance evidence; [Rust conventions](rust-conventions.md) holds the
reusable rules.

| Crate  | Finding                                                                                                                                                                                            | Decision                                                                                                                                                                   |
| ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| CLI    | Parallel `args/` and `commands/` trees split every command type from its execution. Archive, thread, and run dispatchers nested archive I/O and rendering inside their arms.                       | Consolidated into `command/`. Parsed types own `run`; dispatch points to those methods. Shared provider setup, retry, output, and configuration retain their own owners.   |
| TUI    | `App::apply` mixed generation checks, result handling, and state mutation in one message match. Browser key handling mixed navigation and query construction in several arms.                      | Extracted one result method per message. Browser and triage keys now dispatch to named actions; selection and query effects have local owners.                             |
| Engine | `retrieve_threads` held keyword, semantic, hybrid, and fallback workflows inside one match. Sync and refresh coordinate archive, clients, requests, and cancellation without one natural receiver. | Extracted search paths and fallback. Keep sync and refresh as workflow functions; review large phase helpers on their own merits.                                          |
| Store  | Archive lifecycle and storage operations already live on `Archive`. Observation application is a long, correctness-sensitive transaction; ordering matches express the replacement policy.         | Retain the `Archive` receiver and ordering matches. Documented helper contracts; review observation phases separately while preserving transaction and completeness rules. |
| GitHub | Transport behavior is on `GitHubClient`. Resource fetch and normalization functions are grouped by provider family; pagination and retry matches are mostly local policy.                          | Keep resource families visible instead of flattening them into one client implementation. Documented scope, pagination, and failure helper contracts.                      |
| Core   | Checked identities and values already own their validation methods. Remaining matches are small value or domain-state mappings.                                                                    | Keep current ownership and document private identity and coverage helpers.                                                                                                 |

## Follow-up audit

The initial ownership pass addressed selected workflows in every crate; it did not establish that
all functions, APIs, and test bodies had received the same review. Completion claims must name the
reviewed surface and its evidence, rather than extrapolating from a crate or area label.

Implemented follow-ups:

- Browser keys dispatch to named actions. `Movement` owns bounded position calculations; each pane
  owns selection, invalidation, and scrolling effects.
- Embedding reuse and replacement use `EmbeddingPolicy`. Refresh materialization has a stage owner
  with repository traversal, document outcomes, and vector batch accounting.
- Search input and cluster setup tests spell out their small scenarios without loops.
- Semantic search uses `SemanticSource` for candidate scope and `SemanticRanking` for bounded
  scores. Candidate availability precedes query embedding; every page uses the same filter scope.
- Changed browser, refresh embedding, and semantic modules import dependencies at their owners.
- Archive diagnostics separate schema validation, lease observation, and durable work queries.
- Reservation uses `ReservedGeneration` for ordering and persistence; staging uses `PageWrite` for
  generation validation, replay comparison, insertion, and count accounting. Commit remains with the
  archive operation.
- Family reuse uses `MembershipExpectation` and `FamilyFreshness` for source clock, head, coverage,
  and membership evidence. No behavioral bool selects its validation policy.
- Global, search, failure, cluster, and member input dispatch names the action instead of performing
  multi-step state changes inside key-match arms.
- Reservation APIs describe ordering, rejected generations, fencing, errors, and canonical effects.
- Semantic-search docs distinguish archived document vectors from the network-generated query
  vector.

Search preparation now separates request conversion, fallback validation, query-client setup, and
read-only execution. The execution owner keeps the request, recipe, and optional client coherent; it
closes the archive before rendering and retains cancellation for engine retrieval. Module docs
correct the stale offline claim for service-backed semantic queries.

Sync preparation and acquisition now belong to its parsed command, with one archive-close point and
typed selection/client/engine failures. Nearby request cases and existing CLI sync contracts cover
selection and output behavior.

Retry now has a command-local request owner and typed boundary failures. Its outer method closes the
archive once before rendering. Sync and retry share a progress owner that drains before result
output and aborts on unexpected drop. Engine retry planning names failure selection, repository
resolution, scope merging, and deterministic ordering; recorded inclusion facts form one concept
instead of behavioral boolean parameters.

The thread SQL projection and update input live with their column mappings in a private module,
using ordinary public items within that implementation boundary. Their docs explain independent
content/evidence positions and the optional evidence advance. Observation children import external
dependencies directly. The source clock columns retain deliberate crate visibility within the public
observation module, with their SQL shape invariant documented.

Terminal repository state now has a `RepositoryPicker` owner. Highlight and applied scope are
separate, and the applied repository is retained across reordered or empty refreshes. This fixes an
index-based targeting risk while keeping stale replies and failed refreshes isolated. A documented
idle/running writer display replaces independent busy, label, and progress fields; it cannot retain
active progress after completion. Discussion list/detail, coverage, failed-run, and cluster
list/detail now have documented owners with direct transition tests. Query tasks own cancellation
and shutdown draining. Local reads share a documented scheduling context, and discussion requests
own filter preparation and retrieval mode. Progress forwarding has a producer/task owner with normal
draining and unexpected-drop cleanup. Query requests document each intent; local cluster transitions
use separate named variants rather than boolean-selected commands. Renderer dependencies name their
defining modules directly. Failure summary selection and ledger projection have dedicated owners and
tests.

Discussion replies now bind generation, offset, and page together. The detail pane uses explicit
empty/loading/ready/failed states and invalidates old selections before beginning another read.
Coverage and failed-run owners document their retained-cache rules. Cluster detail clears another
cluster's members before a pending read, preventing keyboard decisions from targeting old data;
same-cluster refresh can retain its cache. Typed query messages document that asynchronous boundary
in their own module. App state is reduced to panel coordination and shared status rather than each
panel's internal mutation protocol.

Remaining review surfaces:

- Broader function and state review beyond the selected traversal and diagnostic slices.
- Review additional store operations beyond the reservation, staging, and freshness slices.
- Restricted visibility and imports routed through remaining parent module aliases. Engine-root
  passthrough exports are removed; child-family freshness now imports from the actual owners.
- Deeper module and item documentation contracts across all crates. All six crate introductions now
  have expanded entry guidance and have been reviewed in rendered Rustdoc.
- Hosted behavior of the refreshed CI actions. Local workflow syntax validation passes.
- Hosted Linux, Intel macOS, and Windows validation; local checks cannot establish those results.

Retain simple domain mappings and linear SQL binding maps when splitting them increases navigation.
Review exceptions on their actual contracts rather than using line counts as proof of completion.

## Completion checklist

These requirements preserve the full maintainer request. A passing compiler or a selected slice is
not evidence for every row. Keep this checklist open until its scope has actually been reviewed.

| Requirement                                              | Current evidence                                                                  | Remaining work                                                                                   |
| -------------------------------------------------------- | --------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| Meaningful small modules, broad shallow navigation       | Six crate module maps and selected vertical slices                                | Inspect current long functions and multi-effect match arms throughout the workspace              |
| Command/state ownership and top-down reading             | Unified CLI command tree; workflow and transaction owners                         | Review remaining CLI and engine orchestration, TUI result application, and presentation branches |
| Domain types for related inputs; no behavioral bools     | Embedding policy and family membership expectations                               | Review every remaining bool parameter and broad signature for an intentional contract            |
| Explicit local imports and deliberate visibility         | No `use super::*`; changed workflow imports name owners                           | Remove remaining parent import preludes and document public-boundary exceptions                  |
| Every application function documented                    | All handwritten production methods have comments, including local trait contracts | Assess comment depth across all items; presence alone does not establish a useful contract       |
| All modules and items teach their role and relationships | Expanded roots, workflow modules, examples, and API contracts                     | Review remaining private type/constant docs, field contracts, module maps, and rendered pages    |
| Linear nearby tests with clear scenarios                 | Split suites and direct keyboard/cluster setup                                    | Inspect remaining scenario branches and fixture burden; preserve meaningful data-driven cases    |
| Current dependencies and tools                           | Full direct/transitive aggressive audit reports no outdated dependencies          | CI actions refreshed against upstream; native tools checked and nightly refreshed                |
| Reusable guidance recorded                               | Linked documentation and Rust conventions guides                                  | Record any additional recurring findings at the owning guide                                     |
| Formatting and local gates                               | Previous follow-up passed every local gate                                        | Rerun focused and workspace gates for each subsequent implementation batch                       |
| Hosted platform evidence                                 | Native smoke/package matrix is configured                                         | Obtain current Linux, Intel macOS, and Windows execution results                                 |

The syntax inventory distinguishes production functions from tests and trait implementations. It
measures actual function bodies, excluding braces in strings. The initial continuation found 38
production functions over 50 body lines and 113 match arms spanning at least five lines. These are
inspection candidates, not defects by themselves: simple error-code tables, DTO construction, and
linear SQL binding maps may remain. The item pass also found undocumented private representations
and policy constants; trait-associated aliases inherit their trait contract. Documentation presence
and line counts do not establish documentation quality.

### Current tool evidence

Checked on 2026-09-29: rumdl 0.2.77 matches the
[upstream release](https://github.com/rvben/rumdl/releases/tag/v0.2.77); actionlint 1.7.12 matches
[its upstream release](https://github.com/rhysd/actionlint/releases/tag/v1.7.12).
The npm registry reports markdownlint-cli2 0.23.3, matching the installed command. Nightly is
refreshed to 2026-09-29, with rustc 1.101.0-nightly and its matching rustfmt. The repository's Rust
1.98.1 toolchain remains its deliberate build and validation baseline.

CI and release workflows now use checkout v7, setup-python v7, upload-artifact v7, and
download-artifact v8. The upstream usage and input contracts preserve this repository's checkout,
Python setup, and zipped artifact transfer:
[checkout](https://github.com/actions/checkout/blob/v7/README.md),
[setup-python](https://github.com/actions/setup-python/blob/v7/README.md),
[upload-artifact](https://github.com/actions/upload-artifact/blob/v7/README.md), and
[download-artifact](https://github.com/actions/download-artifact/blob/v8/README.md).
Actionlint passes both workflow files. Hosted execution remains separate evidence.

## Bounded remaining implementation work

The remaining cleanup is eight batches followed by one acceptance pass. This replaces the earlier
open-ended follow-up targets; the requirements in the completion checklist remain the acceptance
criteria. Completed CLI embedding/refresh preparation, deterministic chunk construction, and
embedding document selection are evidence for this inventory, not new future tasks.

### 1. Embedding execution — implemented

Selection and deterministic chunks have local owners. `batches` owns request grouping and response
order, `scheduling::BatchScheduler` owns bounded workers and outcome dispatch, and
`execution::EmbeddingWriter` owns service identity, fenced persistence, and lease release. Fatal
exits now abort and drain outstanding requests before fence release, with direct cleanup cases and
existing partial-success/retry integration evidence. Broad signature and documentation acceptance
still belong to batch 7; this phase does not claim to complete the workspace cleanup.

### 2. Cluster construction — implemented

Implemented: `ClusterBuildLease` owns renewal, cooperative interruption, and release ordering. Three
nearby lease cases and generation integration establish error preservation, release, child cleanup,
and caller-token isolation. CLI build preparation now belongs to the parsed arguments, with
canonical identity/policy conversion cases and archive-close-before-presentation ordering. Refresh
traversal now has a stage owner and direct outcome-policy cases. Engine evidence preparation now has
a repository-scoped snapshot and named generation projection. Store input preparation and durable
identity matching now have named modules and local row contracts. Overlap ranking now uses named
membership evidence with six direct assignment cases. Transactional writes now have an application
owner, with a visible single archive commit and linear SQL bind maps retained at their private
owner.

Review CLI `command/cluster/build`, engine `clustering/build` and `refresh/clusters`, and store
`clusters/generation`. Finish when preparation, analysis, and generation persistence have coherent
owners and the writer lease/transaction boundaries remain explicit. Preserve deterministic proposal
ordering, decision application, and existing generation fencing.

### 3. Search — implemented

Implemented: hybrid fusion has named identity-union and per-discussion evidence owners, coupled
semantic evidence, explicit projection/order operations, and local formula documentation. Ranked
retrieval now has its own coordinator and a validated window owner. Named semantic and hybrid
projections keep scoring/fusion separate from acquisition and fallback. Keyword and semantic helpers
import actual dependency owners; scoring limits live beside their scoring implementation.

The coordinator retains a linear acquisition sequence: validate window, load optional keyword
candidates, obtain semantic evidence, classify fallback, read coverage, and project. These explicit
I/O inputs do not require an application-context wrapper. Workspace validation passes. Broader
documentation and test review remain in their respective bounded batches.

### 4. Acquisition — implemented

Implemented: provider traversal and reserved persistence have separate modules, terminal scan
outcomes distinguish cancellation and failure, metadata completion names staging/application/ledger
resolution, and store finalization has a validated terminal state with named cursor and write
phases. Replay tests show both acquisitions explicitly. Direct store cases protect pending-cursor
rejection, empty terminal-page completion, and superseded-generation isolation.

Retained: the scan loop's linear fetch/apply/cursor sequence, explicit fenced/unfenced single-call
dispatch, and metadata reservation's linear ordering. Their contracts remain locally visible;
forwarding wrappers would increase navigation. Final workspace gates pass. Broader item docs,
visibility, and test-suite review remain in the bounded workspace batches.

### 5. Store operations — implemented

Implemented: aggregate coverage and checked bucket accumulation, document persistence phases,
individual timeline projection, diagnostic consistency/effect contracts, member-decision writes,
canonical selection phases, and member-detail enrichment/role precedence. These retain their SQL,
transaction/read ordering, and public output contracts.

Dismissal/restoration now has named state and audit phases with archive-owned validation, fencing,
and commit. Final store acceptance/gates pass. Broader visibility/import seams and suite fixture
complexity stay in the workspace conventions/test batches. A direct public-archive restoration
scenario now checks dismissal clearing, retained member projections, and ordered durable audit
reasons. Final gates pass with that scenario included.

Retained with reason: SQL column/bind projections, diagnostic counter reads, archive-status
assembly, and simple event/family mappings are linear statements of their owning read/write
contract. Splitting each field or label into forwarding helpers would increase navigation.
Member-target resolution keeps its ordered repository, source-thread, and current-membership
validation together. Audit insertion's wide signature maps explicit SQL columns; representation
review remains in the workspace batch.

### 6. CLI and TUI presentation — implemented

Review CLI detail/archive summaries and TUI coverage, cluster detail, search editing, and event-loop
policy. Finish when presentation decisions and state-changing dispatch have named local owners,
selection cannot target stale data, and meaningful rendering behavior is covered. Preserve the
configured output contracts and terminal cancellation behavior.

Completed presentation slices: CLI timeline event wording and archive-status sections; TUI archive
coverage sections, cluster list/detail projections, and the owned terminal event loop. Embedding
output owns its summary and representative-failure query; TUI timeline has named event projections.
CLI thread/search pages have owned summaries, row methods, and a shared coverage/continuation
footer. Browser input and search editing retain their existing named transitions after review.
Store/engine identities and JSON output remain unchanged.

CLI cluster page/detail coordinators now delegate named cluster rows, detail heading, and member
rows. Run detail has a separate module with heading, job, and failure projections. Empty-page,
coverage/continuation, and empty-run output cases supplement the existing offline and process
contracts. Label mappings remain exhaustive local policy, and genuinely linear format projections
stay together rather than splitting each field into a helper.

Implementation inventory is addressed; focused scenarios and all local workspace gates pass. Broader
fixture/snapshot adequacy and documentation-depth review remains in batches 7 and 8.

### 7. Workspace conventions and documentation

Make one complete pass over the existing six crates' modules, items, function signatures, imports,
and visibility. Review documentation depth, remaining parent import preludes, behavioral boolean
parameters, broad signatures, and representation placement. Finish with each finding fixed or an
explicit justified exception, usable module introductions, item contracts at their owning level, and
an accurate module map. Record recurring rules in the linked guides. Documentation presence alone
does not satisfy this pass.

The fresh initial inventory covers 204 production module files across six crates (core 15, store 49,
CLI 35, TUI 32, engine 59, GitHub 14). It finds no missing handwritten production function comments,
but 48 introductions are under ten lines and still need content review. These are review signals,
not proof of documentation quality. Restricted visibility and crate-internal coverage/query preludes
remain explicit review targets; do not expose SQL resources or implementation types just to replace
`pub(crate)` mechanically.

The refreshed inventory after the ownership splits covers 210 production module files (core 15,
store 50, CLI 35, TUI 32, engine 59, GitHub 19). There are 44 introductions below ten lines and no
missing handwritten production function comments under the syntax inventory's exclusions. These
counts remain inspection signals: test/example exclusions and documentation presence do not prove
contract depth. Core identity/value, GitHub wire/acquisition, store checkpoint/health/lease, and
keyword/ranking contracts have received targeted review; the complete pass remains open.

The first core review covered coverage and provider extensions: stale marking became a named
operation, and extension-object validation, rejected-value retention, replacement/null semantics,
and sorted field access gained concrete contracts. Subsequent dispositions below record additional
reviewed surfaces; the complete item-depth review remains open. Observation accessors now explain
source/acquisition context and archive-local sequence meaning. Document hashing, vector validation,
and timestamp precision have explicit named regression scenarios; vector contracts distinguish shape
validation from model compatibility. Core Clippy and strict private-item Rustdoc pass after these
changes.

The targeted conventions pass has since reviewed the previously short module introductions across
CLI inspection/cluster commands, presentation views, engine inspection/refresh/clustering/error, and
store coverage/finalization/build inputs. It corrected inaccurate effect and completeness claims
rather than using line counts as completion evidence. Shared discussion filter arguments now have a
CLI owner, and shared engine query adapters live in a private module with ordinary public functions.
These findings are fixed. Visibility dispositions are recorded below; remaining item-depth
candidates still require review.

Engine restricted-visibility review now leaves two deliberate seams: the enumeration scan context
and executor bridge, and embedding-client retry classification. Enumeration shares already reserved
coordinates and a caller-owned fence with sync; exposing that bridge would require public callers to
reproduce coordinator invariants. Retry classification is internal adapter policy on an error type
publicly re-exported for reporting. Source comments document both exceptions. Shared query, scoring,
clock, and provider-failure policy have private module owners with ordinary public helpers. This
disposition closes the engine visibility candidates. Subsequent paragraphs record the other crates'
visibility dispositions.

Core and TUI contain no restricted-visibility declarations. CLI retains one environment-name
predicate shared with configuration: its spelling-only contract is documented and a separate
single-function module would add navigation without reducing context. GitHub retains two methods on
the public client for internal typed GraphQL adapters: bounded POST and enterprise endpoint
derivation. Publishing them would expose arbitrary request/protocol setup as acquisition API. Their
source contracts document the exception and shared transport safeguards. This disposes of non-store
visibility candidates; the following paragraph records the completed store visibility review.

Store visibility candidates are now disposed of. Observation SQL, coverage projection, and shared
query predicates have private owners and direct imports. Remaining restrictions preserve archive
pool capabilities, lifecycle validation, the embedded migration catalog and raw-pool operations, and
transaction-local lease checks. Publishing those seams would let callers bypass explicit
opening/migration or fenced archive writes. Their source contracts and linked conventions explain
these exceptions. The complete store test suite passes after the ownership moves, including
integration and documentation scenarios.

### 8. Tests

Review the existing suites for scenario loops, branches, opaque behavior helpers, distance from the
code, and weak assertions. Finish with straightforward named scenarios, construction fixtures whose
setup is clear, appropriate nearby/separate suites, and focused rendering snapshots where they
establish observable behavior. Retain genuine complete-catalog/property checks with their purpose
explained. Do not rewrite every assertion simply to introduce rstest or insta.

The cross-crate loop inventory now leaves fixed SQLite sidecar cleanup in store, engine, and CLI
integration fixtures, plus the core fixture-catalog traversal. Sidecar loops are resource cleanup,
not scenario selection; inspected store/engine helpers document that exception. Catalog loops verify
all dynamically discovered payloads and declared references, so fixed parameterized cases would
weaken coverage. Catalog payload hygiene, reference integrity, and scenario invariant coverage now
have separate named tests; all four catalog checks pass. TUI scope/action/cutoff assertions and task
cleanup have received targeted review, and the complete TUI and GitHub suites pass. Store
observation and search suites also use direct owner imports, documented setup effects, and stronger
identity/version assertions. Ordering contracts have separate named scenarios. This evidence does
not close the remaining suite-quality pass. CLI process fixtures and sync scenario imports have
since received direct-owner cleanup. The sync suites also show real acquisition requests and
operations directly at all 24 former helper call sites, eliminating positional family-selection
flags and hidden workflow execution.

The refreshed production function inventory has two bodies above fifty lines: store error-code
mapping and repository page traversal. The error mapping is exhaustive constant classification,
retained together for local review. Traversal keeps requested URL, cycle detection, provider page,
application, cursor advance, and terminal detection in execution order; `ScanPersistence` already
owns durable mutations. Further extraction would scatter the two live cursor facts across another
owner without reducing policy. Its function contract now explains individual parent commits and
which failures can leave durable work. These dispositions cover the two length candidates, not all
match-arm, broad-signature, or documentation-depth candidates.

### Remaining engine match-arm dispositions

The refreshed match inventory was inspected at each owning operation. These decisions distinguish
multi-step behavior from wrapped single calls or explicit domain values:

- Scoring and hybrid fusion timestamp arms each perform one comparison; retain direct field access
  and the visible sort/tie-break policy.
- Enumeration persistence and reservation arms each select one fenced or unfenced archive operation.
  Retain explicit capability selection; forwarding methods would repeat those same inputs.
- Cluster member decisions each call one named archive transition before shared release. Retain
  exclude/include policy at dispatch.
- Ranked retrieval's hybrid arm calls its named operation, with a local assertion that keyword
  candidates were loaded by the preceding mode preparation. Retain this internal prerequisite.
- Keyword fallback exclusion names cancellation/configuration/concurrency errors in one predicate.
  Retain the exhaustive local exclusion policy rather than hiding individual variants behind
  helpers.
- Review preparation constructs one ready-state value from the acquired head and reserved sequence.
  Retain the construction beside the missing-head transition; it performs no additional effects.
- Sync scope arms construct explicit policy values. Retain visible scope order and closed-watermark
  facts rather than adding forwarding constructors for three-field literals.
- Refresh embedding dispatch calls the named repository embedding operation or constructs one failed
  stage. Retain the configured-service decision and its failure value together.
- Cluster release previously combined clock acquisition, archive release, and lost-fence translation
  in a match/closure chain. A named release operation now owns that behavior; result precedence
  stays with completion and has direct already-released regression cases.

Embedding-client backoff now belongs to `wait_to_retry`, with explicit budget and cancellation
contracts. A one-shot failure/success protocol case and direct boundary cases cover its behavior.
These dispositions cover the inspected engine matches, not the remaining signature and
item-documentation review.

### Remaining store match-arm dispositions

Every remaining store arm in the refreshed multiline inventory has a local disposition:

- Coverage freshness keeps family-specific comparison policy visible: complete comment count versus
  advertised count, or acquired versus current review head. Each branch is one comparison.
- Lease diagnostics decodes three named columns from one optional row or uses the empty-row
  defaults. Keep the column contract at the query rather than adding a helper that merely forwards
  the row.
- Source clocks map into the three SQL columns together. Retain the explicit missing/valid/invalid
  encodings and the rejected empty spelling beside each other.
- Archive creation/opening success arms construct one archive from validated pools and metadata.
  Failure arms explicitly close the pool they own before returning the error. Keep ownership and
  failure cleanup visible at the lifecycle operation; a constructor wrapper would hide no policy.
- Cluster member decisions map include/exclude to three persisted values. Retain this fixed mapping
  so SQL state and audit event names can be checked together.
- Timeline tie-breaks produce one kind rank and stable identity string. The review-thread-comment
  identity includes both parent review-thread and child provider ID; retain that uniqueness policy.
- Observation ordering calls the named comparison operation, with absent evidence treated as the
  first observation. Retain the single comparison and visible absence rule.
- Observation coverage maps declared completeness into the domain state using one set of observation
  coordinates. Retain the direct value projection. Child-family incomplete application previously
  mixed state assembly and writing; its named operation now owns that assembly instead.
- Run failures optionally call the named repository-row lookup before inserting the scoped failure.
  Keep the optional identity lookup at the transaction owner; no additional effect is hidden there.

This closes the inspected store match-arm findings. Broad signatures, item contract depth, and test
scenario adequacy remain separate acceptance items.

### CLI, TUI, and GitHub match-arm dispositions

- CLI startup errors now call named configuration/runtime presenters. Optional credential fallback
  calls a named anonymous-access operation. Archive status and doctor success paths have their own
  named presenters, including unhealthy-report exit policy.
- CLI/TUI timeline review-thread branches construct the existing source-fact context and invoke its
  named presentation method. Retain explicit path/resolution/outdated facts; this performs no I/O or
  state transition. Review-comment context construction follows the same projection rule.
- Refresh cluster summaries invoke the named stage/detail projections. Retain the closure that
  adapts the repository slice; it hides no work or state.
- Cluster build dispatch invokes its named command method with resolved settings and cancellation.
  Decision result rendering invokes the shared success renderer with one three-field output value.
  These are long single calls, not embedded command implementations.
- GitHub setup errors map to stable code/message/status triples before shared rendering. Retain the
  complete local table so cancellation status and credential/endpoint/adapter boundaries are
  visible.
- TUI thread-query dispatch constructs the existing coherent request value and calls its named read
  starter. Retain that assembly because the action already supplies exactly those request facts.
- GitHub success dispatch now delegates typed JSON decoding to the bounded response owner, with
  malformed/shape/pagination cases. No GitHub multiline match-arm findings remain in the inventory.

These decisions close the refreshed multiline match inventory across the six crates. They do not
close the independent broad-signature, documentation-depth, or full test-quality passes.

### Core broad-signature dispositions

- `Observation::new` receives six independent evidence/acquisition facts and constructs their
  existing coherent owner. Keep those facts explicit; a wrapper would neither validate generic
  payload completeness nor prove archive sequence reservation.
- `Document::new` receives source identity, recipe, rendered title/text, deduplication text, and
  source time. Keep supplied rendering facts explicit because core does not render recipes. Hashing
  moved from a five-parameter free function to the document's existing expected-hash query, shared
  by construction and store validation. A fixed digest case protects version-one field encoding.
- `ThreadId::new` combines checked repository identity, opaque provider ID, and local number. Keep
  these distinct typed inputs; the result already names their relationship, and construction cannot
  verify provider identity/number correspondence without acquisition evidence.
- The conservative syntax inventory also flags `ProviderData::insert` because it counts `self`. Its
  two actual inputs are a field name and JSON value; retain the normal map operation.

This disposes of the core broad-signature candidates. The remaining crates and documentation-depth
review still require their own evidence.

### Refresh embedding ownership and signatures

`refresh::embed_repositories` now lives beside repository traversal and stage reporting in the
embedding adapter. The coordinator imports that owner directly; the former six-argument forwarding
layer is removed. CLI embedding and composed refresh retain the same public operation and report.

Retain the public operation's explicit archive, repository selection, service, document recipe,
embedding policy, and cancellation inputs. They describe independent boundary choices; wrapping them
in a second context would duplicate the private execution owner without reducing caller facts. The
coordinator's `refresh` entry likewise accepts independently prepared services and a validated
request rather than inventing an application-wide context. Private execution fields now distinguish
request selection from deduplicated order, sync-only progress, stage failures, and document
failures. These dispositions cover those two entry signatures, not all refresh or engine signatures.

### CLI cluster dispatch signature review

Cluster command dispatch no longer expands configuration into embedding settings and recipe through
an intermediate `execute` method. The parsed command's `run` owns interruption and directly selects
the named operation. Build alone consumes configuration during preparation and passes a narrow
`ClusterBuildRequest` to the engine. Removing the forwarding method reduces both signature width and
navigation without introducing another context type. Read and decision operations still receive only
their archive path, output mode, and selected arguments. Other CLI signature candidates remain under
review.

### CLI credential and process presentation signatures

Retain `GitHubCredentialSettings::resolve_token`'s host and cancellation inputs: settings already
own lookup policy, while host selects helper arguments and cancellation belongs to the invocation.
`run_credential_process` retains executable, argument slice, timeout, and cancellation. This private
boundary executes one command directly and independently controls wait duration and interruption; a
one-use argument bag would add a type without eliminating a policy choice. Its contract now names
the post-spawn timed region and the limits of kill-on-drop cleanup.

Root result/error renderers retain explicit output mode, command label, data or diagnostic, human
renderer, and selected exit status. These are distinct presentation choices, not fields of a domain
request. The typed store and engine adapters remain small conversion boundaries; engine cancellation
retains status 130. Progress startup likewise takes a command label, output mode, and verbosity
because advisory delivery is selected independently from terminal result status.

Credential tests use direct environment values, avoiding process-global mutation. Subprocess cases
remain Unix-specific direct invocations with explicit timeout/cancellation; they establish returned
categories, not descendant cleanup or provider authentication. Portable variable spelling now uses
three named rstest cases instead of one assertion bundle. This disposes of the inspected credential,
root-renderer, and progress-start signatures; other CLI candidates remain open.

### CLI local decision execution

Dismiss, restore, exclude, include, and canonical handlers now show archive opening, the named
engine mutation, archive closing, and presentation as a linear sequence. The renderer receives a
completed typed result and `ClusterDecisionOutput`; it no longer accepts an opaque mutation future
or separate acknowledgment fields. This removes hidden side effects and one broad helper signature
without an extra context type. Handler inputs retain the explicit target, optional member/reason,
path, and output policy; member identity and rationale are independent user choices. The renderer's
output, command label, acknowledgment, and result remain independent presentation inputs.

### CLI local read and run dispatch signatures

Cluster listing now belongs to `ClusterListArgs::run_list`; its `into_request` converts the parsed
selection before archive opening, rather than unpacking four loose filter values throughout the
handler. The engine request remains the existing domain boundary. Cluster show retains ID, path, and
output as independent inputs because it has no argument object with additional behavior.

Thread list/show and run list/show retain their small associated handlers: path and output govern
the process boundary, while request/selector/limit/ID govern the selected read. Archive operations
remain visible and handles close before rendering. Run retry validates its positive ID before
creating interruption ownership, converts selected family arguments once into `RetryRequest`, and
delegates acquisition to that request. Retain path, output, verbosity, ID, and family selection at
that conversion boundary; no extra wrapper would remove a caller decision. Archive command dispatch
similarly retains path and output beside `self`. These dispositions cover the inspected local read
and run dispatch signatures rather than the remaining service-backed CLI preparations.

### Offline CLI scenario locality

The former offline test combined eight commands and validated archive status only after the entire
sequence. Each command now has a named scenario with its own construction, visible process call,
output assertions, and before/after reported-state comparison. Shallow sibling files group keyword,
advanced FTS, thread selection, validation, and human presentation; the shared fixture file only
constructs the known repository/observation and releases resources. This preserves existing output
expectations while making failures and nonmutation evidence local to each selected command.

### Configuration and search policy scenarios

Explicit valid configuration and invalid recipe rejection now have separate process cases. The
invalid case asserts that archive creation never occurs, rather than reusing an archive initialized
by the preceding success path. Semantic unavailability, permitted hybrid fallback, and invalid
keyword fallback each have their own archive, command, and diagnostic/mode assertions in the
`search_policy` sibling. Construction remains explicit and contains no scenario-selection branches.
The configuration and search policy modules are 157 and 146 lines respectively; suite-level fixture
and boundary documentation explains what process evidence proves.

### Archive lifecycle and empty run-history scenarios

Retain creation followed by status as one identity round-trip: status must expose the exact archive
ID allocated by init. Current-schema migration and healthy-doctor expectations now use separate
named scenarios with independent archives. This isolates maintenance failures from creation output.
Empty run listing and missing-run show/retry likewise have separate named cases. Their names and
module introduction now describe the deliberately absent ledger rather than implying recorded-run
coverage. The show/retry parameterization selects command data only; execution and assertions remain
linear and local, including command identity, failure status, typed code, and absent success data.

### CLI process-suite review disposition

The existing process suites have now been inspected across archive, configuration, search policy,
status, sync, run history, process flags, cluster commands, and offline queries. Mixed independent
scenarios are split; retained sequences verify a real round-trip (creation/status, build/list).
Construction fixtures expose their data/effects and do not execute the scenario workflow. Fixed
sidecar cleanup loops remain resource release, and parameterized cases select explicit command data.

Missing-archive status now asserts command identity and absent success data. Empty `sync --all` uses
an explicit empty archive fixture and a truthful zero-work name. Its lease-conflict scenario retains
visible current-clock construction because the competing process checks expiry itself. Process help
now reflects the selected TUI feature, the noninteractive TUI case is feature-gated, and version
output matches exact package metadata. This closes the inspected CLI process-scenario quality
findings; CLI unit suites and the remaining cross-crate suite review are separate work.

### CLI configuration and parsing unit scenarios

Recipe acceptance now uses named original-body and enriched cases, each parsing one input and
asserting one expected domain recipe. Unknown recipes inspect the parse diagnostic; invalid service
endpoint and capacity scenarios assert `InvalidEmbeddings` rather than accepting any error. Global
path parsing compares typed paths directly without optional UTF-8 conversion. Filter parser
documentation identifies the production flattened arguments and workflow-owned default sort.

Retain coordinated global-option and explicit-filter assertions together: each inspects one parsed
value and its complete conversion, without running another workflow. Sync/refresh parsing cases
retain a local `let`/`else` variant check because the test must establish the selected enum before
inspecting its payload; this is a typed assertion rather than scenario control flow. This review
covers configuration, global parsing, and shared filters; other CLI unit owners remain open.

### Prepared acquisition unit scenarios

Embedding preparation/projection cases keep static client configuration and stage data visible,
without credentials, archives, or provider requests. The partial projection now contains nonzero
chunk counts, batch failure, document failure, and stage failure, with direct identity/status/count
assertions. Retain this linear whole-projection case together: splitting it would repeat
construction without clarifying an independent policy. Cancellation-code and fallback-diagnostic
cases remain separate named tests.

Refresh cases retain coherent request preparation assertions and now distinguish caller selection
order from engine execution order. Typed cause preservation checks the exact `InvalidSyncScope`
variant rather than any engine error plus display wording. Sync conversion cases document empty
all-repository scope and independent family flags. This disposes of the inspected embedding,
refresh, and sync preparation suite findings, not every CLI unit or cross-crate suite.

### CLI embedding and refresh report suites

Retain named failure-precedence cases and the explicit stage-to-exit table: each establishes one
policy without hidden workflow execution. The shared embedding output fixture now follows all its
callers and documents fixed identity, present empty report, and diagnostic-free construction.
Failure values have direct defining-owner imports and names before assignment/insertion, avoiding
wrapped struct literals inside mutation chains. Message selection remains independent of supplied
stage status; the fixture does not infer outcome policy from diagnostics.

Refresh summary tests keep whole rendered strings because ordering, omission, counts, and suffixes
are the observable contract of one projection. Their module now distinguishes report-selected order
from engine scheduling and remaining work from missing stage details. These dispositions close the
inspected embedding and refresh-summary unit suites; other report owners remain open.

### Provider setup cause assertions

Credential, endpoint, and adapter setup causes now have separate linear tests that downcast the
source and check the exact error variant. The former table compared only display strings despite
claiming typed preservation; those assertions could not establish the intended boundary. Explicit
cancellation remains a separate no-source case. The reusable rule now distinguishes source-type
contracts from presentation wording in the Rust conventions.

### CLI unit-suite review disposition

The CLI unit owners have now been inspected across configuration, credentials, global/filter
parsing, sync/refresh/embedding preparation, cluster build, provider setup, progress lifetime,
envelopes, and the page/run/detail/timeline/cluster/embedding/refresh report projections. Small
inline report suites remain near implementation; their introductions now name fixture facts,
observable contracts, and evidence limits. Success-envelope serialization compares the whole value
as failure serialization already did. Progress docs distinguish empty-channel lifecycle checks from
buffered-event rendering. Cluster build retains its pure parsed-value fixture below callers.

Together with the preceding scenario and assertion fixes, this closes the inspected existing CLI
unit-suite quality findings. It does not close cross-crate test review, the CLI signature inventory,
or production documentation-depth acceptance. New tests remain subject to the linked conventions.

### Core identity scenario locality

Accepted authority normalization uses named enterprise, IPv6, and default-port cases with explicit
authority and origin expectations. Provider-text rejection, zero numeric identities, abbreviated SHA
rejection, full SHA normalization, repository JSON round-trip, and host deserialization now have
independent cases instead of one broad assertion bundle. The round-trip consumes its encoded value
without an unnecessary clone. JSON rejection checks data classification and identity-validation
wording; it does not claim that serde preserves the original typed cause. Imports name the defining
identity module. Other core suites remain under review.

### Core timestamp boundary scenarios

Malformed source parsing and invalid calendar-date deserialization have separate named tests.
Parsing compares the typed timestamp error; JSON rejection checks serde's data classification and
the validation message, without claiming preservation of a typed cause. The offset-equivalence
round-trip stays together because its assertions describe one instant's normalized representation.
Archive range and precision retain named boundary cases with fixed times rather than clock fixtures.
Other core suites remain under review.

### Core and engine local-suite dispositions

Core content, vector, coverage, observation, timestamp, identity, outcome, and document suites now
have direct scenarios and local orientation. Coverage holds evidence family constant and compares
whole JSON values. Vector encoding checks exact bytes. Timestamp parsing and JSON rejection are
separate boundaries; deterministic hash tests retain explicit version-one encoding evidence.
Round-trips remain together when they establish one value representation. Fixture-catalog traversal
remains a deliberate complete-catalog check rather than fixed scenario iteration.

Engine scoring separates best-chunk relevance, identity ties, zero-score exclusion, and cancellation
already signaled before entry. Document recipes separate ordering, review inclusion, bot exclusion,
case normalization, and stale-family eligibility; construction lives in a shallow sibling module.
Chunking separates repeatability from explicit whitespace, UTF-8, and empty-input cases. Batching
checks counts, retained order, and multibyte byte accounting. Construction helpers calculate no
expected rankings or results, and chunk/batch contracts identify caller validation responsibilities.

Graph title/kind thresholds have their own shallow suite, including high-confidence same-kind
acceptance without title overlap. Reference membership is explicit. Proposal cases supply retained
edges directly, while scheduler and cluster lease tests inspect real resource cleanup and error
precedence. Sync/refresh accounting and recorded selection use direct named values and transitions.
Cosine tests distinguish exact axis results from tolerance-based arithmetic. These dispositions do
not close workflow integration review.

### Engine embedding-client suite dispositions

Protocol acceptance matches the entire JSON request, including model, configured dimensions, float
encoding, and ordered inputs. It checks exact response count before indexed vector comparisons and
reads request history separately from assertions. Redirect rejection proves one source attempt and
no destination requests. Retry keeps two explicit mock expectations; direct backoff cases assert
budget exhaustion and cancellation without HTTP requests. Configuration helpers construct settings
only. Named response-validation and error-classification matrices already compare exact errors
without scenario loops or hidden operations and are retained.

Cluster workflow integration now separates complete creation, partial preservation, and unmatched
endpoint rejection into shallow scenario owners. Fixtures construct repository/discussion/document
values only. Each scenario shows observation reservation/application, fenced document and chunk
writes, lease release, and engine calls. The partial case keeps its dependent complete baseline and
replacement together: preservation requires evidence that the group previously existed. Complete
creation additionally checks the listed active lifecycle and member count. Namespace rejection uses
persisted vectors without an unrelated prior build.

Enumeration integration now has shallow partial, replay, and construction owners. Page failure
compares the complete persisted scan with its report and the exact continuation URL, verifies the
retained canonical repository and issue, and distinguishes complete parent coverage from missing
comments. Replay proves both initial identities/titles, compares their entire discussion payloads
after reacquisition, and checks terminal continuation/failure absence plus persisted scan equality.
Client and repository-response fixtures perform no enumeration or archive mutation. The dependent
before/after replay remains one linear contract.

Engine workflow integration remains open for sync suites. Sync read projections need inspection for
hidden retrieval boundaries and assertions that establish only counts. Remaining observation
assertion acceptance and broad documentation/API acceptance are separate review work.

### Store lifecycle and cluster-suite dispositions

Store local completion, summary, timeline, role, lease-conversion, and generation-matching suites
have documented checked-value boundaries. Matching compares entire assignment maps. Timeline inverse
comparisons establish one ordering contract; summary accumulation establishes one bucket total.

Lifecycle integration has shallow access, diagnostics, migration, and infrastructure owners.
Opening, current migration, missing-file rejection, healthy probes, and damaged history are
independently named. Diagnostics compare persisted lease owner, fence, expiry, and the entire named
family/count sequence. Raw pools explicitly disable creation; archive operations and assertions
remain visible.

Lease integration proves stale reservation leaves sequence one available to its successor and stale
release cannot revoke that successor. Scan completion compares entire checkpoints before and after
rejection. Terminal-page acceptance remains one coherent linear transition; superseded generations
have a separate scenario. Fixed times are used for checkpoint state and current time for live
fencing.

Cluster persistence has shallow decision-retention, partial-preservation, complete-retirement,
canonical-validation, coverage-validation, and construction owners. Generation values name coverage
and counts, and every observation reservation/application is visible. Invalid complete coverage
checks its specific error and absence of persisted clusters. Replacement cases compare omitted group
identity/lifecycle separately from human decisions. Fixtures perform no archive operations.

The restoration regression remains one linear public-operation contract: dismissal, restoration,
retained membership, and read-only audit events. Its construction helpers supply fixed values only;
all fenced mutations and cleanup remain visible. Splitting this coherent before/after regression
would duplicate its transitions without improving understanding of restoration.

Remaining test-quality work covers store observation/search integration acceptance and engine
workflow suites. Completed local, lifecycle, and cluster findings are removed from that remaining
inventory. Broad API/convention and production documentation-depth acceptance remain independent.

### Store observation and search-suite dispositions

Observation integration has shallow high-water, integrity, ordering, comment rollback, and
review-thread rollback owners. Replay and tied-conflict rejection are independent scenarios with
retained-title assertions. Malformed-clock rejection also checks that the original title survives.
Fixed revision-sequence comparisons live in ordering tests without unrelated database setup.
High-water transitions remain one linear contract because earlier acquisitions constrain later
selection. Rollback assertions require the database error and injected trigger diagnostic before
checking retained membership, coverage, and staged retry where exercised.

Child publication has independent replay, partial, incomplete-empty, head-snapshot, and supersession
suites. Review and review-thread snapshots use named rstest inputs with identical visible public
operations. Each replacement scenario supplies its complete baseline without unrelated family
writes. Construction fixtures own payload values; creation, registration, reservation, staging, and
completion remain explicit. Store uses the existing workspace-selected rstest version as a test-only
dependency. Remaining observation assertion acceptance is independent of these resolved grouping
findings.

Search integration has shallow pagination, filtering, scope coverage, status counts, read-only
preservation, repository lookup, query validation, detail, FTS, and migration owners. Fixtures build
values and query settings; archive mutations and reads remain visible. Scope coverage and status
compare the entire ordered family summary, including every missing/incomplete/complete count, rather
than unnamed vector indices. Malformed syntax has an indexed thread as a precondition: an empty FTS
index can return no rows without exercising SQLite expression validation. Detail compares complete
comment identity/payload and chronological event values, identifies both issue-applicable coverage
families, and rejects PR-only evidence. FTS replacement remains one coherent before/after contract
with queries separated from assertions; its name no longer implies a rollback test. Migration
retains visible linear schema teardown because that setup explains the backfill boundary, replaces
eight repeated bookkeeping deletes with one bounded statement, and reports the controlled fixture's
historical-schema limits. These inspected search cases have concrete dispositions; observation
assertion acceptance and engine client/workflow review remain open.

### Acceptance pass and stopping rules

- Reconcile every explicit maintainer requirement against current source and recorded evidence.
- Give every inspection candidate one disposition: fixed or retained with a concrete reason. Line
  counts trigger inspection, not mandatory extraction or repeated work on newly named helpers.
- Newly noticed aesthetic opportunities do not extend these batches unless they violate an agreed
  requirement or are consequences of the changes being made.
- Update the module map, guidance, and completion checklist to describe the actual implementation;
  remove completed work from the remaining inventory.
- Run focused checks and all applicable local workspace formatting, lint, test, build, and strict
  public/private documentation gates on the final tree. Confirm dependency/tool evidence remains
  current without turning the cleanup into another dependency redesign.
- Keep new features, generic frameworks, and deferred distribution out of this scope.

Hosted Linux, Intel macOS, and Windows results remain separate validation evidence. Workflow syntax
and local gates do not establish those results. No remote publication or hosted execution has been
performed as part of this cleanup.
